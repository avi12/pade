// Auto-retry on API error: when an agent CLI turn stops abruptly on a transient
// server-side failure — "API Error: 529", an overloaded_error, a 500/502/503, a
// dropped connection — nudge it to pick the work back up rather than leaving it
// wedged at the prompt. Every RETRY_MS we type "continue" into the stuck session
// while its context window still has room; once the window is nearly full another
// retry would only stall again, so we hand off to a fresh successor agent (the
// auto-handoff flow) and stop. The loop ends the moment the session is working
// again (the nudge took, or it self-recovered) or its tab is gone. Opt-out rides
// the same prefs.autoResume switch as usage-limit auto-resume. This module owns
// detection and what a retry does; the timers, note and scan are the shared
// lib/stores/recoveryLoop, and the app shell supplies sessions and the handoff
// through `RecoveryHost` and drives the scan from a component `$effect`.

import { pty } from "@/lib/bridge";
import { measuredContextPercentage } from "@/lib/stores/context.svelte";
import { createRecoveryLoop } from "@/lib/stores/recoveryLoop.svelte";
import type { RecoveryHost } from "@/lib/stores/recoveryLoop.svelte";
import { sessionStatus } from "@/lib/stores/sessions.svelte";
import { SessionStatus } from "@/lib/types";
import type { AgentSession } from "@/lib/types";
import { SvelteMap } from "svelte/reactivity";

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

/** The auto-retry machinery, scoped to one app shell. The shell calls `check()`
 *  from a `$effect` and `dispose()` on destroy; `note` is the status line to show
 *  while a retry is pending ("" when idle). */
export function createApiErrorRetry(host: RecoveryHost) {
  const loop = createRecoveryLoop({
    host,
    hits,
    firstDelay: RETRY_MS,
    tick: retry,
    pendingNote: session => `${session.agent.label} stopped on an API error — retrying…`
  });

  // One retry tick, RETRY_MS after the last. A session halted by an API error
  // sits idle at its prompt (`ready`), so the moment it reports `working` again
  // the nudge took hold (or it self-recovered) and we stop; an `exited` PTY can't
  // be retried, so we give up. Otherwise, while the context window has room we
  // type "continue"; once it's nearly full another retry would only stall again,
  // so we hand off to a fresh agent and stop.
  async function retry(session: AgentSession) {
    const id = session.id;
    const status = sessionStatus(id);
    const recovered = status === SessionStatus.enum.working;
    const dead = status === SessionStatus.enum.exited;
    if (!loop.isOpen(id) || recovered || dead) {
      loop.forget(id);
      return;
    }

    const percentage = measuredContextPercentage(id);
    const hasRoom = percentage === null || percentage < host.thresholdPercentage();
    if (!hasRoom) {
      loop.forget(id);
      host.forceHandoff(session);
      return;
    }

    await pty.write({
      id,
      data: "continue\r"
    }).catch(() => {});
    loop.schedule({
      session,
      delay: RETRY_MS
    });
  }

  return {
    /** Status line shown while a retry is pending ("" when idle). */
    get note() {
      return loop.note;
    },
    check: loop.check,
    dispose: loop.dispose
  };
}
