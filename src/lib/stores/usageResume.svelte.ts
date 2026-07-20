// Usage-limit auto-resume: when an agent CLI stops because its usage window is
// exhausted ("5-hour limit reached ∙ resets 3am"), schedule the session to
// continue the moment the window resets — by typing "continue" into the same
// session when its context window still has room, or by handing off to a fresh
// agent (the auto-handoff flow) when it doesn't. Opt-out via prefs.autoResume;
// the machinery mirrors lib/stores/handoff: this module owns detection,
// scheduling and teardown, the app shell supplies sessions and the handoff
// through `ResumeHost` and drives the scan from a component `$effect`.

import { pty, usage } from "@/lib/bridge";
import { CONTEXT_HANDOFF_PCT } from "@/lib/context-level";
import { measuredContextPct } from "@/lib/stores/context.svelte";
import type { AgentSession } from "@/lib/types";
import { SvelteDate, SvelteMap, SvelteSet } from "svelte/reactivity";

/** The usage windows an agent CLI can exhaust — which one names the reset time. */
export const LimitWindow = {
  session: "session",
  weekly: "weekly"
} as const;
export type LimitWindow = (typeof LimitWindow)[keyof typeof LimitWindow];

/** A window is only treated as truly exhausted when the account API agrees —
 *  the sniffer can hit a stale message replayed from scrollback history. */
const EXHAUSTED_PCT = 95;
/** Fire a little after the reset, so the window has actually rolled over. */
const RESET_BUFFER_MS = 90_000;
/** When no reset time is known yet (API offline), probe again this often. */
const RETRY_MS = 5 * 60_000;

// The CLI's own stop message is the trigger. "limit reached" (never the softer
// "approaching…" warning), with the window named before it; an inline clock
// ("resets 3am", "resets at 3:30pm") is parsed as the schedule when present.
const LIMIT_REACHED_RE = /\b(?:(5-hour|session|usage|weekly)\s+limit reached|limit reached)\b/i;
const WEEKLY_HINT_RE = /weekly/i;
const RESET_CLOCK_RE = /resets?\s*(?:at\s*)?(\d{1,2})(?::(\d{2}))?\s*(am|pm)/i;

interface LimitHit {
  window: LimitWindow;
  /** Reset time parsed from the CLI message itself, when it printed one. */
  inlineResetAt: number | null;
  scheduled: boolean;
}

const hits = new SvelteMap<string, LimitHit>();

/** The next wall-clock occurrence of an "3am" / "3:30pm" style clock time. */
export function nextOccurrence({ hour, minute, meridiem, now }: {
  hour: number;
  minute: number;
  meridiem: string;
  now: number;
}): number {
  const isPastNoon = meridiem.toLowerCase() === "pm";
  const hour24 = (hour % 12) + (isPastNoon ? 12 : 0);
  const candidate = new SvelteDate(now);
  candidate.setHours(hour24, minute, 0, 0);

  if (candidate.getTime() <= now) {
    candidate.setDate(candidate.getDate() + 1);
  }

  return candidate.getTime();
}

/** Parse the reset clock out of a limit message, or null when it names none. */
export function parseResetClock({ text, now }: {
  text: string;
  now: number;
}): number | null {
  const clock = text.match(RESET_CLOCK_RE);
  if (!clock) {
    return null;
  }

  const hour = Number(clock[1]);
  const minute = Number(clock[2] ?? 0);
  const withinClockRange = hour >= 1 && hour <= 12 && minute <= 59;
  if (!withinClockRange) {
    return null;
  }

  return nextOccurrence({
    hour,
    minute,
    meridiem: clock[3],
    now
  });
}

/** Whether a chunk of agent output is the CLI's limit-reached stop message —
 *  and which window it names. Null for everything else (including the softer
 *  "approaching usage limit" warning). */
export function parseLimitHit({ text, now }: {
  text: string;
  now: number;
}): Omit<LimitHit, "scheduled"> | null {
  if (!LIMIT_REACHED_RE.test(text)) {
    return null;
  }

  return {
    window: WEEKLY_HINT_RE.test(text) ? LimitWindow.weekly : LimitWindow.session,
    inlineResetAt: parseResetClock({
      text,
      now
    })
  };
}

/** Feed a chunk of a session's PTY output through the limit sniffer. A TUI
 *  repaints its stop message on every frame, so a session already marked (or
 *  already scheduled) is left alone. */
export function observeUsageLimit({ id, chunk }: {
  id: string;
  chunk: string;
}): void {
  if (hits.has(id)) {
    return;
  }

  const hit = parseLimitHit({
    text: chunk,
    now: Date.now()
  });
  if (hit) {
    hits.set(id, {
      ...hit,
      scheduled: false
    });
  }
}

/** Forget a session's limit state when it ends. */
export function dropUsageLimit(id: string): void {
  hits.delete(id);
}

/** What the app shell provides. */
export interface ResumeHost {
  sessions: () => AgentSession[];
  /** Whether the user opted out via prefs.autoResume. */
  isOptedOut: () => boolean;
  /** Hand a session off to a fresh agent now (the auto-handoff flow). */
  forceHandoff: (session: AgentSession) => void;
}

/** The auto-resume machinery, scoped to one app shell. The shell calls
 *  `check()` from a `$effect` and `dispose()` on destroy; `note` is the status
 *  line to show while a resume is pending ("" when idle). */
export function createUsageResume(host: ResumeHost) {
  let note = $state("");
  const timers = new SvelteMap<string, ReturnType<typeof setTimeout>>();

  function clearTimer(id: string) {
    const timer = timers.get(id);
    if (timer !== undefined) {
      clearTimeout(timer);
      timers.delete(id);
    }
  }

  // The window's reset instant. The account API is both the health gate and
  // the preferred clock: its `resets_at` is a to-the-second UTC stamp (probed
  // live: "2026-07-17T08:20:00+00:00"), where the CLI's own "resets 3am" names
  // a bare local hour. A window the API reports healthy means the sniffed
  // message was stale scrollback (a remounted terminal replays history) —
  // signalled with NaN so the caller drops the hit. The inline clock only
  // carries the schedule when the API is unreachable (offline, no token);
  // null means "not knowable yet" and the caller probes again later.
  async function resolveResetAt(hit: LimitHit): Promise<number | null> {
    const account = await usage.account().catch(() => null);
    // The CLI names a session/weekly window; find the matching one in the
    // account's generic window list by kind (LimitWindow ⊆ UsageWindowKind).
    const window = account?.windows.find(candidate => candidate.kind === hit.window);
    if (window && window.utilization < EXHAUSTED_PCT) {
      return Number.NaN;
    }

    const stamp = window?.resetsAt ? Date.parse(window.resetsAt) : Number.NaN;
    if (!Number.isNaN(stamp)) {
      return stamp;
    }

    return hit.inlineResetAt;
  }

  async function resume(session: AgentSession) {
    hits.delete(session.id);
    timers.delete(session.id);
    note = "";

    const stillHere = host.sessions().some(s => s.id === session.id);
    if (!stillHere) {
      return;
    }

    // Room left in the context window → the same session just continues.
    // Nearly full → resuming would stall again within a few turns, so hand
    // off to a fresh agent instead (it reads the handoff doc and carries on).
    const pct = measuredContextPct(session.id);
    const hasRoom = pct === null || pct < CONTEXT_HANDOFF_PCT;
    if (hasRoom) {
      try {
        await pty.write({
          id: session.id,
          data: "continue\r"
        });
      } catch {
        // The session may have exited between scheduling and this resume; ignore.
      }

      return;
    }

    host.forceHandoff(session);
  }

  async function schedule({ session, hit }: {
    hit: LimitHit;
    session: AgentSession;
  }) {
    const resetAt = await resolveResetAt(hit);
    // NaN = the account window is healthy; the message was stale. Drop it.
    if (Number.isNaN(resetAt)) {
      hits.delete(session.id);
      return;
    }

    const delay = resetAt === null
      ? RETRY_MS
      : Math.max(0, resetAt - Date.now()) + RESET_BUFFER_MS;
    if (resetAt === null) {
      // Reset time unknown (offline, no token): probe again in a while.
      timers.set(
        session.id, setTimeout(async () => {
          timers.delete(session.id);
          await schedule({
            session,
            hit
          });
        }, delay)
      );
      return;
    }

    const clock = new Intl.DateTimeFormat(undefined, { timeStyle: "short" }).format(resetAt);
    note = `${session.agent.label} hit its usage limit — resuming at ${clock}.`;
    timers.set(
      session.id, setTimeout(async () => {
        await resume(session);
      }, delay)
    );
  }

  // Scan for freshly limited sessions and schedule their resume; prune state
  // for sessions that no longer exist.
  function check() {
    if (host.isOptedOut()) {
      return;
    }

    const alive = new SvelteSet(host.sessions().map(s => s.id));
    for (const id of [...hits.keys(), ...timers.keys()]) {
      if (!alive.has(id)) {
        clearTimer(id);
        hits.delete(id);
      }
    }

    for (const session of host.sessions()) {
      const hit = hits.get(session.id);
      if (hit && !hit.scheduled) {
        hits.set(session.id, {
          ...hit,
          scheduled: true
        });
        schedule({
          session,
          hit
        });
      }
    }
  }

  function dispose() {
    for (const timer of timers.values()) {
      clearTimeout(timer);
    }

    timers.clear();
  }

  return {
    /** Status line shown while a resume is pending ("" when idle). */
    get note() {
      return note;
    },
    check,
    dispose
  };
}
