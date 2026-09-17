// Usage-limit auto-resume: when an agent CLI stops because a usage window is
// exhausted ("You've hit your session limit · resets 5:30pm"), wait — never
// nudge. Every "continue" typed into a limited session only hits the limit again,
// and cancels Claude Code's own wait for the reset. The session is continued
// once, the moment it can run again: its window resets, or usage credits are
// turned on for the account — which the CLI itself never notices. The account
// usage endpoint answers both, so a waiting session re-reads it every POLL_MS.
// "continue" goes into the same session while its context window has room; the
// auto-handoff flow takes over when it doesn't. Opt-out via prefs.autoResume.
// This module owns detection and what a look at a waiting session does; the
// timers, note and scan are the shared lib/stores/recoveryLoop, and the app shell
// supplies sessions and the handoff through `RecoveryHost` and drives the scan
// from a component `$effect`.

import { stripAnsi } from "@/lib/ansi";
import { pty, usage } from "@/lib/bridge";
import { measuredContextPercentage } from "@/lib/stores/context.svelte";
import { createRecoveryLoop } from "@/lib/stores/recoveryLoop.svelte";
import type { RecoveryHost } from "@/lib/stores/recoveryLoop.svelte";
import { sessionStatus } from "@/lib/stores/sessions.svelte";
import { PROMPT_SUBMIT } from "@/lib/terminal-input";
import { CreditsState, SessionStatus } from "@/lib/types";
import type { AccountUsage, AgentSession } from "@/lib/types";
import { SvelteDate, SvelteMap } from "svelte/reactivity";

/** The usage windows an agent CLI can exhaust — which one names the reset time. */
export const LimitWindow = {
  session: "session",
  weekly: "weekly"
} as const;
export type LimitWindow = (typeof LimitWindow)[keyof typeof LimitWindow];

/** What the account says about a session stopped on a usage limit. */
export const LimitVerdict = {
  /** The window is exhausted and no credits carry the session — keep waiting. */
  blocked: "blocked",
  /** The account can't say (offline, expired token) and no known reset has passed. */
  unknown: "unknown",
  /** It can run again: its window reset, or usage credits now carry it. */
  runnable: "runnable",
  /** The account never showed the session blocked — the message was a stale
   *  repaint of scrollback. */
  stale: "stale"
} as const;
export type LimitVerdict = (typeof LimitVerdict)[keyof typeof LimitVerdict];

/** The prompt that picks a stopped agent's work back up. */
export const CONTINUE_PROMPT = `continue${PROMPT_SUBMIT}`;

/** A window counts as exhausted from here — the account is the judge, since the
 *  sniffer also sees stale stop messages repainted from scrollback. */
const EXHAUSTED_PERCENTAGE = 95;
/** When the account can't be read, the reset clock alone decides — a little after
 *  it, so the window has actually rolled over. */
const RESET_BUFFER_MS = 90_000;
/** How often a waiting session re-reads its account. The backend caches that read
 *  for ~3 minutes, so the usage endpoint itself is never hit more often. */
const POLL_MS = 60_000;
/** A settled stop message (stale, or its session continued) keeps repainting on
 *  screen; sniffs of that session are ignored this long so each repaint doesn't
 *  re-read the account. */
const SETTLED_QUIET_MS = 5 * 60_000;

// The CLI's own stop message is the trigger, in each phrasing Claude Code has
// used — "You've hit your session limit · resets 5:30pm", "Usage limit reached ·
// continuing automatically at 5:30pm", "5-hour limit reached ∙ resets 3am", and
// "You've hit your monthly spend limit · … your session limit resets 5:30pm" (the
// window is out AND the credits carrying it hit their cap) — plus the rate_limit /
// HTTP 429 error a limited request fails with. Never the softer "approaching…"
// warning, nor a fast-mode limit. A fullscreen TUI moves the cursor instead of
// printing spaces, so with the escapes stripped words can arrive run together:
// every gap is optional whitespace.
const LIMIT_STOP_RE = new RegExp(
  "(?<reachedWindow>5-hour|session|usage|weekly)?\\s*limit\\s*reached"
    + "|hit\\s*your\\s*(?:(?<hitWindow>5-hour|session|usage|weekly)\\s*|monthly\\s*spend\\s*)?limit"
    + "|error\\s*type\\s*rate_limit|rate_limit_error|http\\s*429",
  "i"
);
const RESET_CLOCK_RE = /resets?\s*(?:at\s*)?(\d{1,2})(?::(\d{2}))?\s*(am|pm)/i;
// An agent TUI shows how to interrupt only while a turn is running — Claude Code's
// "esc to interrupt", Codex's "Esc to interrupt", opencode's "esc interrupt" — in
// its status rows at the foot of the screen. Only those rows are read, so the same
// words quoted in the transcript above never count.
const TURN_RUNNING_RE = /esc\s*(?:to\s*)?interrupt/i;
const STATUS_ROWS = 8;

interface LimitHit {
  window: LimitWindow;
  /** The best-known reset instant: the account's to-the-second stamp once read,
   *  else the clock the CLI message printed, else null. */
  resetAt: number | null;
  /** The account has shown this session blocked — from then on a healthy window
   *  or credits turning on mean it can run again, not that the message was stale. */
  confirmed: boolean;
  scheduled: boolean;
}

const hits = new SvelteMap<string, LimitHit>();
/** Sessions whose stop message is settled, and until when their sniffs are ignored. */
// eslint-disable-next-line svelte/prefer-svelte-reactivity -- read per output chunk, never rendered
const quietUntil = new Map<string, number>();
/** Waiting sessions whose screen, as last painted, shows a turn running. PADE's
 *  session status can't tell this: it counts every byte, and a stuck session still
 *  ticks a background workflow's timer and spins its window title, so it can read
 *  `working` indefinitely. The screen says whether the agent is actually mid-turn. */
// eslint-disable-next-line svelte/prefer-svelte-reactivity -- written per screen sample, never rendered
const turnRunningOnScreen = new Map<string, boolean>();

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

/** Whether a chunk of agent output carries a usage-limit stop — the classifier
 *  API-error auto-retry defers to, so the two never claim the same session. */
export function isUsageLimitStop(text: string): boolean {
  return LIMIT_STOP_RE.test(stripAnsi(text));
}

/** Whether a chunk of agent output is the CLI's usage-limit stop message — and
 *  which window it names, with the reset clock it prints. Null for everything else
 *  (including the softer "approaching usage limit" warning). */
export function parseLimitHit({ text, now }: {
  text: string;
  now: number;
}): Pick<LimitHit, "window" | "resetAt"> | null {
  const plain = stripAnsi(text);
  const stop = plain.match(LIMIT_STOP_RE);
  if (!stop) {
    return null;
  }

  const windowName = stop.groups?.reachedWindow ?? stop.groups?.hitWindow ?? "";
  const namesWeekly = windowName.toLowerCase() === LimitWindow.weekly;
  return {
    window: namesWeekly ? LimitWindow.weekly : LimitWindow.session,
    resetAt: parseResetClock({
      text: plain,
      now
    })
  };
}

function matchingWindow({ hit, account }: {
  hit: Pick<LimitHit, "window">;
  account: AccountUsage | null;
}) {
  return account?.windows.find(candidate => candidate.kind === hit.window);
}

/** The account's to-the-second reset stamp for the hit's window, when it names one. */
function accountResetAt({ hit, account }: {
  hit: Pick<LimitHit, "window">;
  account: AccountUsage | null;
}): number | null {
  const stamp = matchingWindow({
    hit,
    account
  })?.resetsAt;
  if (!stamp) {
    return null;
  }

  const parsed = Date.parse(stamp);
  return Number.isNaN(parsed) ? null : parsed;
}

/** Judge a stopped session against its account: still blocked, runnable again
 *  (its window reset, or credits carry it), stale (the account never showed it
 *  blocked), or unknown (the account can't say — the reset clock decides). */
export function assessLimit({ hit, account, now }: {
  hit: Pick<LimitHit, "window" | "resetAt" | "confirmed">;
  account: AccountUsage | null;
  now: number;
}): LimitVerdict {
  const window = matchingWindow({
    hit,
    account
  });
  const creditsCarry = account?.credits === CreditsState.enum.available;
  const accountCanTell = window !== undefined || creditsCarry;
  if (!accountCanTell) {
    const resetPassed = hit.resetAt !== null && now >= hit.resetAt + RESET_BUFFER_MS;
    return resetPassed ? LimitVerdict.runnable : LimitVerdict.unknown;
  }

  const windowExhausted = window !== undefined && window.utilization >= EXHAUSTED_PERCENTAGE;
  if (windowExhausted && !creditsCarry) {
    return LimitVerdict.blocked;
  }

  return hit.confirmed ? LimitVerdict.runnable : LimitVerdict.stale;
}

// Record a stop message found in a session's output, unless the session is already
// waiting or its last message was just settled — a TUI repaints its stop message
// on every frame.
function sniffStop({ id, text }: {
  id: string;
  text: string;
}) {
  const now = Date.now();
  const settled = (quietUntil.get(id) ?? 0) > now;
  if (hits.has(id) || settled) {
    return;
  }

  const sniffed = parseLimitHit({
    text,
    now
  });
  if (sniffed) {
    hits.set(id, {
      ...sniffed,
      confirmed: false,
      scheduled: false
    });
  }
}

/** Whether a rendered screen shows the agent mid-turn, read from its status rows. */
export function showsTurnRunning(screen: string): boolean {
  const statusRows = screen.trimEnd().split("\n").slice(-STATUS_ROWS).join("\n");
  return TURN_RUNNING_RE.test(statusRows);
}

/** Feed a chunk of a session's PTY output through the limit sniffer. */
export function observeUsageLimit({ id, chunk }: {
  id: string;
  chunk: string;
}): void {
  sniffStop({
    id,
    text: chunk
  });
}

/** Feed a session's rendered screen (xterm viewport rows) through the sniffer. The
 *  screen carries what the wire can split — a TUI skips unchanged cells with cursor
 *  moves — and, for a waiting session, whether a turn is running right now. */
export function observeUsageLimitScreen({ id, text }: {
  id: string;
  text: string;
}): void {
  if (hits.has(id)) {
    turnRunningOnScreen.set(id, showsTurnRunning(text));
    return;
  }

  sniffStop({
    id,
    text
  });
}

/** Whether a session is stopped on a usage limit this module is waiting out. */
export function hasUsageLimit(id: string): boolean {
  return hits.has(id);
}

/** Forget a session's limit state when it ends. */
export function dropUsageLimit(id: string): void {
  hits.delete(id);
  quietUntil.delete(id);
  turnRunningOnScreen.delete(id);
}

/** Status line shown while a session waits out its limit. */
function waitingNote({ session, resetAt }: {
  session: AgentSession;
  resetAt: number | null;
}): string {
  const agent = session.agent.label;
  if (resetAt === null) {
    return `${agent} hit its usage limit — resuming when it resets, or once usage credits can cover it.`;
  }

  const clock = new Intl.DateTimeFormat(undefined, { timeStyle: "short" }).format(resetAt);
  return `${agent} hit its usage limit — resuming at ${clock}, or sooner if usage credits can cover it.`;
}

/** The account the session's agent bills against. An IPC failure reads as "the
 *  account can't say", which leaves the decision to the reset clock. */
async function readAccount(session: AgentSession): Promise<AccountUsage | null> {
  try {
    return await usage.accountFor({ agent: session.agent.id });
  } catch {
    return null;
  }
}

/** The auto-resume machinery, scoped to one app shell. The shell calls
 *  `check()` from a `$effect` and `dispose()` on destroy; `note` is the status
 *  line to show while a session waits ("" when idle). */
export function createUsageResume(host: RecoveryHost) {
  const loop = createRecoveryLoop({
    host,
    hits,
    firstDelay: 0,
    tick: assess,
    forgetState: id => turnRunningOnScreen.delete(id)
  });

  // Forget the wait and ignore the same message repainting for a while.
  function settle(id: string) {
    loop.forget(id);
    quietUntil.set(id, Date.now() + SETTLED_QUIET_MS);
  }

  // Continue a session that can run again — unless it already is: the agent's own
  // auto-continue at the reset, or the user, got there first, and a "continue"
  // typed into a running turn would queue a stray one behind it. Room left in the
  // context window → the same session continues; nearly full → it would stall
  // again within a few turns, so hand off to a fresh agent instead.
  async function resume(session: AgentSession) {
    const turnRunning = turnRunningOnScreen.get(session.id) ?? false;
    const gone = sessionStatus(session.id) === SessionStatus.enum.exited;
    settle(session.id);

    if (turnRunning || gone) {
      return;
    }

    const percentage = measuredContextPercentage(session.id);
    const hasRoom = percentage === null || percentage < host.thresholdPercentage();
    if (!hasRoom) {
      host.forceHandoff(session);
      return;
    }

    await pty.write({
      id: session.id,
      data: CONTINUE_PROMPT
    });
  }

  // Still blocked, or the account can't say: remember what the account confirmed
  // (and its reset stamp) and look again in POLL_MS.
  function keepWaiting({ session, hit, account, verdict }: {
    session: AgentSession;
    hit: LimitHit;
    account: AccountUsage | null;
    verdict: LimitVerdict;
  }) {
    const waiting = {
      ...hit,
      confirmed: hit.confirmed || verdict === LimitVerdict.blocked,
      resetAt: accountResetAt({
        hit,
        account
      }) ?? hit.resetAt
    };
    hits.set(session.id, waiting);
    loop.note = waitingNote({
      session,
      resetAt: waiting.resetAt
    });
    loop.schedule({
      session,
      delay: POLL_MS
    });
  }

  // One look at a waiting session: read its account and act on the verdict —
  // settle a stale message, continue a runnable session, or keep waiting.
  async function assess(session: AgentSession) {
    const id = session.id;
    if (!loop.isOpen(id) || !hits.has(id)) {
      loop.forget(id);
      return;
    }

    const account = await readAccount(session);
    const hit = hits.get(id);
    const droppedMeanwhile = loop.disposed || hit === undefined;
    if (droppedMeanwhile) {
      return;
    }

    const verdict = assessLimit({
      hit,
      account,
      now: Date.now()
    });
    if (verdict === LimitVerdict.stale) {
      settle(id);
      return;
    }

    if (verdict === LimitVerdict.runnable) {
      await resume(session);
      return;
    }

    keepWaiting({
      session,
      hit,
      account,
      verdict
    });
  }

  return {
    /** Status line shown while a session waits ("" when idle). */
    get note() {
      return loop.note;
    },
    check: loop.check,
    dispose: loop.dispose
  };
}
