// Usage pacing: a working agent that burns through a usage window well before it
// resets is paused, and continued once the window has room for it again - so the
// account doesn't sit locked out for hours at the end of a window. Each paced
// window (the 5-hour session, the weekly cap) gets a budget line that starts at
// PACE_FLOOR_PERCENTAGE when the window opens and climbs to 100% at its reset. An
// agent at or past the line is asked to stop at its next safe point; as time
// passes the line rises past its usage and PADE types "continue" - so a heavy run
// is slowed into bursts, and a run far ahead of the line is stopped outright.
// Near a reset (RESET_GRACE_MS) a window never pauses anyone: it is about to come
// back anyway. Weekly is paced the same way, so a near-reset session window only
// carries on while the weekly one has room. Usage credits that carry work past a
// spent window turn pacing off for that account - the user chose to pay to keep
// going. A user who restarts a paused agent by hand wins: that session is not
// paused again until it falls back under the line. Opt-out via prefs.autoPace.
// This module owns the policy and the per-session phase; the app shell supplies
// sessions through `PacingHost` and drives the scan from a component `$effect`.

import { submitPastedPrompt } from "@/lib/agent/submit-prompt";
import { Command, sendMessage } from "@/lib/ipc/messaging";
import { agentReportsBusy } from "@/lib/stores/agent/agentActivity.svelte";
import { sessionStatus } from "@/lib/stores/session/sessions.svelte";
import { CONTINUE_PROMPT, hasUsageLimit, readAccount } from "@/lib/stores/usage/usageResume.svelte";
import { CreditsState, SessionStatus, UsageWindowKind } from "@/lib/types";
import type { AccountUsage, AgentSession, UsageWindow } from "@/lib/types";
import { resetCountdown, usageResetTime } from "@/lib/usage/usage-groups";
import { afterDelay } from "@/lib/util/delay";
import { formatPercent } from "@/lib/util/format";
import { SvelteMap } from "svelte/reactivity";

/** Where every paced window's budget line starts, the moment the window opens. */
export const PACE_FLOOR_PERCENTAGE = 60;
const FULL_PERCENTAGE = 100;
/** This close to its reset a window paces no one - it will be back shortly. */
export const RESET_GRACE_MS = 10 * 60_000;
/** A paused session continues only once the line is this far above its usage,
 *  so it doesn't pause again a minute after continuing. */
const RESUME_MARGIN_PERCENTAGE = 2;
/** How often working and paused sessions are checked against their account. The
 *  backend caches that read for ~3 minutes, so the endpoint is never hit faster. */
const POLL_MS = 60_000;

const HOUR_MS = 60 * 60_000;
const DAY_MS = 24 * HOUR_MS;

/** The windows paced, with how long each runs and what the prompt calls it. A
 *  window of another kind (a per-model cap, extra usage) is never paced. */
const PACED_WINDOWS: Partial<Record<UsageWindowKind, {
  lengthMs: number;
  name: string;
}>> = {
  [UsageWindowKind.enum.session]: {
    lengthMs: 5 * HOUR_MS,
    name: "5-hour"
  },
  [UsageWindowKind.enum.weekly]: {
    lengthMs: 7 * DAY_MS,
    name: "weekly"
  }
};

/** Where a paced session is: asked to stop and still finishing its step, stopped
 *  and waiting for room, or restarted by the user while over the line. */
const PacePhase = {
  Pausing: "pausing",
  Paused: "paused",
  Overridden: "overridden"
} as const;
type PacePhase = (typeof PacePhase)[keyof typeof PacePhase];

/** The usage a window may have spent by `now`: its budget line, or null when the
 *  window isn't paced - an unpaced kind, an unknown reset, or a reset within
 *  RESET_GRACE_MS. */
export function paceLine({ window, now }: {
  window: Pick<UsageWindow, "kind" | "resetsAt">;
  now: number;
}) {
  const paced = PACED_WINDOWS[window.kind];
  const timeLeft = usageResetTime(window.resetsAt) - now;
  const isPaced = paced !== undefined && Number.isFinite(timeLeft) && timeLeft > RESET_GRACE_MS;
  if (!isPaced) {
    return null;
  }

  const elapsedFraction = Math.min(1, Math.max(0, 1 - timeLeft / paced.lengthMs));
  return PACE_FLOOR_PERCENTAGE + (FULL_PERCENTAGE - PACE_FLOOR_PERCENTAGE) * elapsedFraction;
}

/** When a window's line next reaches `utilization` plus the resume margin - or
 *  its grace period begins, whichever comes first. */
function resumeTime({ window, now }: {
  window: UsageWindow;
  now: number;
}) {
  const paced = PACED_WINDOWS[window.kind];
  const resetAt = usageResetTime(window.resetsAt);
  if (paced === undefined) {
    return now;
  }

  const target = window.utilization + RESUME_MARGIN_PERCENTAGE;
  const targetFraction = (target - PACE_FLOOR_PERCENTAGE) / (FULL_PERCENTAGE - PACE_FLOOR_PERCENTAGE);
  const lineReachesTarget = resetAt - paced.lengthMs * (1 - targetFraction);
  return Math.max(now, Math.min(lineReachesTarget, resetAt - RESET_GRACE_MS));
}

/** Judge an account's pace. `isPaused` adds the resume margin, so a paused
 *  session stays paused until the line has pulled clearly ahead. The window that
 *  holds the session longest is the one reported, with when it lets go. */
export function assessPace({ account, now, isPaused }: {
  account: AccountUsage | null;
  now: number;
  isPaused: boolean;
}) {
  const areCreditsCarrying = account?.credits === CreditsState.enum.available;
  if (!account || areCreditsCarrying) {
    return null;
  }

  const margin = isPaused ? RESUME_MARGIN_PERCENTAGE : 0;
  let binding: {
    window: UsageWindow;
    resumeAt: number;
  } | null = null;
  for (const window of account.windows) {
    const line = paceLine({
      window,
      now
    });
    const isOverLine = line !== null && window.utilization + margin >= line;
    if (!isOverLine) {
      continue;
    }

    const resumeAt = resumeTime({
      window,
      now
    });
    if (binding === null || resumeAt > binding.resumeAt) {
      binding = {
        window,
        resumeAt
      };
    }
  }

  return binding;
}

/** The request that stops a working agent at its next safe point. */
export function pauseRequest({ window, now }: {
  window: UsageWindow;
  now: number;
}) {
  const name = PACED_WINDOWS[window.kind]?.name ?? window.label;
  const countdown = resetCountdown({
    iso: window.resetsAt,
    now
  });
  return `PADE usage pacing: ${formatPercent(window.utilization)} of this account's ${name} usage is spent and it resets ${countdown}, so work pauses here to spread the rest out. Finish the step you're on, leave the work easy to pick up, then stop and wait - PADE will send "continue" when there's room again.`;
}

interface PaceState {
  phase: PacePhase;
  window: UsageWindow;
  resumeAt: number;
}

const paced = new SvelteMap<string, PaceState>();

/** Forget a session's pacing when it ends. */
export function dropUsagePacing(id: string) {
  paced.delete(id);
}

const RESUME_CLOCK = new Intl.DateTimeFormat(undefined, {
  weekday: "short",
  hour: "numeric",
  minute: "2-digit"
});

function pacingNote({ session, state }: {
  session: AgentSession;
  state: PaceState;
}) {
  const name = PACED_WINDOWS[state.window.kind]?.name ?? state.window.label;
  return `${session.agent.label} paused to pace its ${name} usage (${formatPercent(state.window.utilization)}) - continuing around ${RESUME_CLOCK.format(state.resumeAt)}`;
}

/** What the app shell provides. */
export interface PacingHost {
  sessions: () => AgentSession[];
  /** Whether the user opted out via prefs.autoPace. */
  isOptedOut: () => boolean;
  /** Whether the auto-handoff flow owns the session right now. */
  isHandingOff: (id: string) => boolean;
}

/** The pacing loop, scoped to one app shell. The shell calls `check()` from a
 *  `$effect` and `dispose()` on destroy; `note` is the status line shown while a
 *  session is paused ("" when none is). */
export function createUsagePacing(host: PacingHost) {
  let timer: ReturnType<typeof setTimeout> | undefined;
  let isDisposed = false;

  // A turn running, by the agent's own report or, for an agent that reports
  // nothing, by its output.
  function isWorking(id: string) {
    return agentReportsBusy(id) || sessionStatus(id) === SessionStatus.enum.working;
  }

  async function pause({ session, window, resumeAt }: {
    session: AgentSession;
    window: UsageWindow;
    resumeAt: number;
  }) {
    paced.set(session.id, {
      phase: PacePhase.Pausing,
      window,
      resumeAt
    });
    await submitPastedPrompt({
      id: session.id,
      text: pauseRequest({
        window,
        now: Date.now()
      }),
      wait: afterDelay
    });
  }

  // Back under the line. A session still finishing its step is left to stop
  // first; a stopped one is continued, unless the user already took it over.
  async function release({ session, state }: {
    session: AgentSession;
    state: PaceState;
  }) {
    const isWorkingNow = isWorking(session.id);
    if (state.phase === PacePhase.Pausing && isWorkingNow) {
      return;
    }

    paced.delete(session.id);
    const shouldContinue = state.phase !== PacePhase.Overridden && !isWorkingNow;
    if (shouldContinue) {
      await sendMessage(Command.ptyWrite, {
        id: session.id,
        data: CONTINUE_PROMPT
      });
    }
  }

  // Still over the line: note the step finishing, or the user taking over.
  function hold({ session, state, window, resumeAt }: {
    session: AgentSession;
    state: PaceState;
    window: UsageWindow;
    resumeAt: number;
  }) {
    const isWorkingNow = isWorking(session.id);
    let phase = state.phase;
    if (phase === PacePhase.Pausing && !isWorkingNow) {
      phase = PacePhase.Paused;
    } else if (phase === PacePhase.Paused && isWorkingNow) {
      phase = PacePhase.Overridden;
    }

    paced.set(session.id, {
      phase,
      window,
      resumeAt
    });
  }

  async function paceSession({ session, account }: {
    session: AgentSession;
    account: AccountUsage | null;
  }) {
    const state = paced.get(session.id);
    const isOwnedElsewhere = hasUsageLimit(session.id) || host.isHandingOff(session.id);
    if (isOwnedElsewhere) {
      paced.delete(session.id);
      return;
    }

    const binding = assessPace({
      account,
      now: Date.now(),
      isPaused: state !== undefined
    });
    if (state === undefined) {
      const shouldPause = binding !== null && isWorking(session.id);
      if (shouldPause) {
        await pause({
          session,
          ...binding
        });
      }

      return;
    }

    const isAccountSilent = account === null;
    if (isAccountSilent) {
      return;
    }

    if (binding === null) {
      await release({
        session,
        state
      });
      return;
    }

    hold({
      session,
      state,
      ...binding
    });
  }

  // One look at every session worth a look - working ones that may need pausing,
  // paced ones that may continue - reading each agent's account once.
  async function scan() {
    const sessions = host.sessions();
    // eslint-disable-next-line svelte/prefer-svelte-reactivity -- one scan's lookup, never rendered
    const open = new Set(sessions.map(session => session.id));
    for (const id of paced.keys()) {
      if (!open.has(id)) {
        paced.delete(id);
      }
    }

    // eslint-disable-next-line svelte/prefer-svelte-reactivity -- one scan's reads, never rendered
    const accounts = new Map<string, Promise<AccountUsage | null>>();
    for (const session of sessions) {
      const isWorthALook = paced.has(session.id) || isWorking(session.id);
      if (!isWorthALook) {
        continue;
      }

      let account = accounts.get(session.agent.id);
      if (account === undefined) {
        account = readAccount(session);
        accounts.set(session.agent.id, account);
      }

      await paceSession({
        session,
        account: await account
      });
    }
  }

  async function tick() {
    timer = undefined;

    if (isDisposed) {
      return;
    }

    if (host.isOptedOut()) {
      paced.clear();
    } else {
      await scan();
    }

    schedule();
  }

  function schedule() {
    if (isDisposed || timer !== undefined) {
      return;
    }

    timer = setTimeout(tick, POLL_MS);
  }

  return {
    /** Status line shown while a session is paused ("" when none is). */
    get note() {
      for (const session of host.sessions()) {
        const state = paced.get(session.id);
        const isWaitingForRoom = state !== undefined && state.phase !== PacePhase.Overridden;
        if (isWaitingForRoom) {
          return pacingNote({
            session,
            state
          });
        }
      }

      return "";
    },
    /** Start the poll; reads nothing reactive, so the `$effect` runs it once. */
    check: schedule,
    dispose() {
      isDisposed = true;
      clearTimeout(timer);
      timer = undefined;
    }
  };
}
