// Auto-retry on API error: when an agent CLI turn stops abruptly on a transient
// server-side failure — "API Error: 529", an overloaded_error, a 500/502/503, a
// dropped connection — nudge it to pick the work back up rather than leaving it
// wedged at the prompt. Every RETRY_MS we type "continue" into the stuck session
// while its context window still has room; once the window is nearly full another
// retry would only stall again, so we hand off to a fresh successor agent (the
// auto-handoff flow) and stop. The loop ends the moment the session is working
// again (the nudge took, or it self-recovered) or its tab is gone. Opt-out rides
// the same prefs.autoResume switch as usage-limit auto-resume. The machinery
// mirrors lib/stores/usageResume: this module owns detection, the retry loop and
// teardown, while the app shell supplies sessions and the handoff through
// `RetryHost` and drives the scan from a component `$effect`.

import { pty } from "@/lib/bridge";
import { CONTEXT_HANDOFF_PCT } from "@/lib/context-level";
import { measuredContextPct } from "@/lib/stores/context.svelte";
import { sessionStatus } from "@/lib/stores/sessions.svelte";
import { SessionStatus } from "@/lib/types";
import type { AgentSession } from "@/lib/types";
import { SvelteMap, SvelteSet } from "svelte/reactivity";

/** How often a stuck session is nudged to continue. */
const RETRY_MS = 30_000;

/** HTTP server-error status codes an abruptly-stopped agent turn surfaces. */
const SERVER_ERROR_STATUS = [500, 502, 503, 529] as const;

/** Phrase signals of an API-side failure. Deliberately NOT "limit reached" — a
 *  usage window running out is auto-resume's job (lib/stores/usageResume). */
const API_ERROR_SIGNALS = [
  "api error",
  "overloaded_error", // the API's 529 payload type — not the bare English word
  "internal server error",
  "bad gateway",
  "service unavailable",
  "connection error",
  "connection reset",
  "econnreset",
  "network error"
] as const;

// A bare status code only counts next to an http/error keyword, so an incidental
// "500 modules built" in ordinary output never trips the sniffer. The trailing
// (?![0-9a-z]) also rejects a unit suffix, so a duration like "response: 500ms"
// (500 milliseconds, not an HTTP status) never counts.
const HTTP_STATUS_LEAD_IN = "(?:http|https|status|error|code|response)";
const API_ERROR_RE = new RegExp(
  `${API_ERROR_SIGNALS.join("|")}`
    + `|${HTTP_STATUS_LEAD_IN}[^0-9a-z]{0,6}(?:${SERVER_ERROR_STATUS.join("|")})(?![0-9a-z])`,
  "i"
);

interface ApiErrorHit {
  scheduled: boolean;
}

const hits = new SvelteMap<string, ApiErrorHit>();

/** Whether a chunk of agent output is a transient API-side stop worth retrying —
 *  an "API Error", an overloaded/500/502/503/529 server error, or a dropped
 *  connection. Never the usage-limit "limit reached" message (auto-resume's). */
export function parseApiError({ text }: { text: string }): boolean {
  return API_ERROR_RE.test(text);
}

/** Feed a chunk of a session's PTY output through the API-error sniffer. A TUI
 *  repaints its error on every frame, so a session already marked (or already
 *  scheduled for retry) is left alone. */
export function observeApiError({ id, chunk }: {
  id: string;
  chunk: string;
}): void {
  if (hits.has(id)) {
    return;
  }

  if (parseApiError({ text: chunk })) {
    hits.set(id, { scheduled: false });
  }
}

/** Forget a session's API-error state when it ends. */
export function dropApiError(id: string): void {
  hits.delete(id);
}

/** What the app shell provides. */
export interface RetryHost {
  sessions: () => AgentSession[];
  /** Whether the user opted out (shares prefs.autoResume with usage auto-resume). */
  isOptedOut: () => boolean;
  /** Hand a session off to a fresh agent now (the auto-handoff flow) — used when
   *  the context window is too full for another retry to get anywhere. */
  forceHandoff: (session: AgentSession) => void;
}

/** Status line shown while a session's retry loop is pending. */
function retryNote(session: AgentSession): string {
  return `${session.agent.label} stopped on an API error — retrying…`;
}

/** The auto-retry machinery, scoped to one app shell. The shell calls `check()`
 *  from a `$effect` and `dispose()` on destroy; `note` is the status line to show
 *  while a retry is pending ("" when idle). */
export function createApiErrorRetry(host: RetryHost) {
  let note = $state("");
  let disposed = false;
  const timers = new SvelteMap<string, ReturnType<typeof setTimeout>>();

  function clearTimer(id: string) {
    const timer = timers.get(id);
    if (timer !== undefined) {
      clearTimeout(timer);
      timers.delete(id);
    }
  }

  // End one session's retry loop: drop its timer + hit, and clear the shared note
  // once nothing else is pending.
  function stop(id: string) {
    clearTimer(id);
    hits.delete(id);

    if (timers.size === 0) {
      note = "";
    }
  }

  function scheduleRetry(session: AgentSession) {
    if (disposed) {
      return;
    }

    note = retryNote(session);
    timers.set(
      session.id, setTimeout(async () => {
        await retry(session);
      }, RETRY_MS)
    );
  }

  // One retry tick, RETRY_MS after the last. A session halted by an API error
  // sits idle at its prompt (`ready`), so the moment it reports `working` again
  // the nudge took hold (or it self-recovered) and we stop; an `exited` PTY can't
  // be retried, so we give up. Otherwise, while the context window has room we
  // type "continue"; once it's nearly full another retry would only stall again,
  // so we hand off to a fresh agent and stop.
  async function retry(session: AgentSession) {
    const id = session.id;
    timers.delete(id);

    const stillHere = host.sessions().some(s => s.id === id);
    if (!stillHere) {
      stop(id);
      return;
    }

    const status = sessionStatus(id);
    const recovered = status === SessionStatus.enum.working;
    const dead = status === SessionStatus.enum.exited;
    if (recovered || dead) {
      stop(id);
      return;
    }

    const pct = measuredContextPct(id);
    const hasRoom = pct === null || pct < CONTEXT_HANDOFF_PCT;
    if (!hasRoom) {
      stop(id);
      host.forceHandoff(session);
      return;
    }

    await pty.write({
      id,
      data: "continue\r"
    }).catch(() => {});
    scheduleRetry(session);
  }

  // Scan for freshly errored sessions and start their retry loop; prune state for
  // sessions that no longer exist.
  function check() {
    if (host.isOptedOut()) {
      return;
    }

    const alive = new SvelteSet(host.sessions().map(s => s.id));
    for (const id of [...hits.keys(), ...timers.keys()]) {
      if (!alive.has(id)) {
        stop(id);
      }
    }

    for (const session of host.sessions()) {
      const hit = hits.get(session.id);
      if (hit && !hit.scheduled) {
        hits.set(session.id, { scheduled: true });
        scheduleRetry(session);
      }
    }
  }

  function dispose() {
    disposed = true;
    for (const timer of timers.values()) {
      clearTimeout(timer);
    }

    timers.clear();
  }

  return {
    /** Status line shown while a retry is pending ("" when idle). */
    get note() {
      return note;
    },
    check,
    dispose
  };
}
