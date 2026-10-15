// Usage pacing: a working agent that burns through a usage window well before it
// resets is paused, so the account doesn't sit locked out for hours at the end of
// a window. Each paced window (the 5-hour session, the weekly cap) gets a budget
// line that starts at PACE_FLOOR_PERCENTAGE when the window opens and climbs to
// 100% at its reset. An agent at or past the line is asked to stop at its next
// safe point, and PADE remembers when it stopped it - per window, across a reload.
// Once the line climbs past its usage the session is ready, and prefs.usagePacing
// says what happens: `continue` picks it straight back up (a heavy run slows into
// bursts), `ask` leaves the call to the user - and, left alone, it continues on
// its own as the window is about to reset (RESET_GRACE_MS before it). Near a reset a window never pauses anyone:
// it is about to come back anyway. Weekly is paced the same way, so a near-reset
// session window only carries on while the weekly one has room. Usage credits
// that carry work past a spent window turn pacing off for that account - the user
// chose to pay to keep going. A user who restarts a paused agent by hand wins:
// that session is not paused again until it falls back under the line. `off`
// paces nothing. This module owns the policy and the per-session phase; the
// app shell supplies sessions through `PacingHost`, drives the scan from a
// component `$effect`, and shows the pause with its Continue button.

import { submitPastedPrompt } from "@/lib/agent/submit-prompt";
import { Command, sendMessage } from "@/lib/ipc/messaging";
import { windowLabel } from "@/lib/ipc/platform";
import { agentReportsBusy } from "@/lib/stores/agent/agentActivity.svelte";
import { sessionStatus } from "@/lib/stores/session/sessions.svelte";
import { CONTINUE_PROMPT, hasUsageLimit, readAccount } from "@/lib/stores/usage/usageResume.svelte";
import { CreditsState, PacingMode, SessionStatus, UsageWindowKind } from "@/lib/types";
import type { AccountUsage, AgentSession, UsageWindow } from "@/lib/types";
import { resetCountdown, usageResetTime } from "@/lib/usage/usage-groups";
import { afterDelay } from "@/lib/util/delay";
import { formatPercent } from "@/lib/util/format";
import { readStoredJson, writeStoredJson } from "@/lib/util/stored-json";
import { SvelteMap } from "svelte/reactivity";
import { z } from "zod";

/** Where every paced window's budget line starts, the moment the window opens. */
export const PACE_FLOOR_PERCENTAGE = 60;
const FULL_PERCENTAGE = 100;
/** This close to its reset a window paces no one - it will be back shortly - and
 *  a session it paused continues on its own. */
export const RESET_GRACE_MS = 10 * 60_000;
/** A paused session is ready only once the line is this far above its usage, so
 *  continuing doesn't pause it again a minute later. */
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
 *  and waiting for room, stopped with room again (the user's call), or restarted
 *  by the user while over the line. */
const PacePhase = z.enum(["pausing", "paused", "ready", "overridden"]);
type PacePhase = z.infer<typeof PacePhase>;

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
function readyTime({ window, now }: {
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
 *  holds the session longest is the one reported: when the line lets it go
 *  (`readyAt`), and when its reset is close enough to continue on its own
 *  (`continueAt`). */
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
    readyAt: number;
    continueAt: number;
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

    const readyAt = readyTime({
      window,
      now
    });
    if (binding === null || readyAt > binding.readyAt) {
      binding = {
        window,
        readyAt,
        continueAt: usageResetTime(window.resetsAt) - RESET_GRACE_MS
      };
    }
  }

  return binding;
}

function windowName(kind: UsageWindowKind) {
  return PACED_WINDOWS[kind]?.name ?? kind;
}

/** The request that stops a working agent at its next safe point. */
export function pauseRequest({ window, now }: {
  window: UsageWindow;
  now: number;
}) {
  const countdown = resetCountdown({
    iso: window.resetsAt,
    now
  });
  return `PADE usage pacing: ${formatPercent(window.utilization)} of this account's ${windowName(window.kind)} usage is spent and it resets ${countdown}, so work pauses here to spread the rest out. Finish the step you're on, leave the work easy to pick up, then stop and wait - you'll be told to continue.`;
}

/** One paced session, as remembered across a reload of its window. */
const PaceState = z.object({
  phase: PacePhase,
  windowKind: UsageWindowKind,
  utilization: z.number(),
  /** When PADE asked the session to stop. */
  pausedAt: z.number(),
  /** When the budget line has room for it again. */
  readyAt: z.number(),
  /** When its window is about to reset and it continues on its own. */
  continueAt: z.number(),
  /** The user chose to have this session continue the moment it has room,
   *  whatever the pacing mode says. */
  isContinuingWhenReady: z.boolean().default(false)
});
type PaceState = z.infer<typeof PaceState>;

const PacedSessions = z.record(z.string(), PaceState);

/** Keyed by window label, like the session snapshot, so a window rebuilt after
 *  a crash finds the pauses of the sessions it re-attaches. */
function pacingStorageKey() {
  return `pade.usage-pacing:${windowLabel()}`;
}

const paced = new SvelteMap<string, PaceState>();

function remember() {
  writeStoredJson({
    key: pacingStorageKey(),
    value: Object.fromEntries(paced)
  });
}

function setPace({ id, state }: {
  id: string;
  state: PaceState;
}) {
  paced.set(id, state);
  remember();
}

function forgetPace(id: string) {
  if (paced.delete(id)) {
    remember();
  }
}

/** Forget a session's pacing when it ends. */
export function dropUsagePacing(id: string) {
  forgetPace(id);
}

const CLOCK = new Intl.DateTimeFormat(undefined, { timeStyle: "short" });
const DAY_AND_CLOCK = new Intl.DateTimeFormat(undefined, {
  weekday: "short",
  hour: "numeric",
  minute: "2-digit"
});

/** A time as said in the note - the clock alone today, with the weekday after. */
function spokenTime({ time, now }: {
  time: number;
  now: number;
}) {
  const isToday = new Date(time).toDateString() === new Date(now).toDateString();
  return isToday ? CLOCK.format(time) : DAY_AND_CLOCK.format(time);
}

/** The status line for a paced session. */
export function pacingNote({ agent, state, isContinuingWhenReady, now }: {
  agent: string;
  state: Pick<PaceState, "phase" | "windowKind" | "utilization" | "pausedAt" | "readyAt" | "continueAt">;
  /** Whether it continues as soon as it has room, not just at the reset. */
  isContinuingWhenReady: boolean;
  now: number;
}) {
  const pausedAt = spokenTime({
    time: state.pausedAt,
    now
  });
  const usage = `${windowName(state.windowKind)} usage (${formatPercent(state.utilization)})`;
  const continuesAt = spokenTime({
    time: isContinuingWhenReady ? state.readyAt : state.continueAt,
    now
  });
  if (state.phase === PacePhase.enum.ready) {
    return `${agent}, paused at ${pausedAt}, has room in its ${usage} again - continues on its own at ${continuesAt}`;
  }

  return `${agent} paused at ${pausedAt} to pace its ${usage} - continues on its own around ${continuesAt}`;
}

/** What the app shell provides. */
export interface PacingHost {
  sessions: () => AgentSession[];
  /** The resolved prefs.usagePacing, read per scan so a Config change applies
   *  without a restart. */
  mode: () => PacingMode;
  /** Whether the auto-handoff flow owns the session right now. */
  isHandingOff: (id: string) => boolean;
}

/** The pacing loop, scoped to one app shell. The shell calls `check()` from a
 *  `$effect` and `dispose()` on destroy, shows `pause` while a session waits, and
 *  calls `continueNow` when the user picks it back up. */
export function createUsagePacing(host: PacingHost) {
  let timer: ReturnType<typeof setTimeout> | undefined;
  let isDisposed = false;

  for (const [id, state] of Object.entries(
    readStoredJson({
      key: pacingStorageKey(),
      schema: PacedSessions
    }) ?? {}
  )) {
    paced.set(id, state);
  }

  // A turn running, by the agent's own report or, for an agent that reports
  // nothing, by its output.
  function isWorking(id: string) {
    return agentReportsBusy(id) || sessionStatus(id) === SessionStatus.enum.working;
  }

  async function pause({ session, window, readyAt, continueAt }: {
    session: AgentSession;
    window: UsageWindow;
    readyAt: number;
    continueAt: number;
  }) {
    const now = Date.now();
    setPace({
      id: session.id,
      state: {
        phase: PacePhase.enum.pausing,
        windowKind: window.kind,
        utilization: window.utilization,
        pausedAt: now,
        readyAt,
        continueAt,
        isContinuingWhenReady: false
      }
    });
    await submitPastedPrompt({
      id: session.id,
      text: pauseRequest({
        window,
        now
      }),
      wait: afterDelay
    });
  }

  // Pick a paused session back up - unless it is already working: a "continue"
  // typed into a running turn would queue a stray one behind it.
  async function resume(id: string) {
    forgetPace(id);

    if (isWorking(id)) {
      return;
    }

    await sendMessage(Command.ptyWrite, {
      id,
      data: CONTINUE_PROMPT
    });
  }

  // Back under the line. A session still finishing its step is left to stop
  // first; one the user took over is released; a stopped one continues at once
  // or waits for the user, as the pacing mode says.
  async function release({ id, state }: {
    id: string;
    state: PaceState;
  }) {
    const isWorkingNow = isWorking(id);
    const isStillStopping = state.phase === PacePhase.enum.pausing && isWorkingNow;
    if (isStillStopping) {
      return;
    }

    const isTakenOver = state.phase === PacePhase.enum.overridden
      || (state.phase === PacePhase.enum.ready && isWorkingNow);
    if (isTakenOver) {
      forgetPace(id);
      return;
    }

    const shouldContinue = host.mode() === PacingMode.enum.continue || state.isContinuingWhenReady;
    if (shouldContinue) {
      await resume(id);
      return;
    }

    setPace({
      id,
      state: {
        ...state,
        phase: PacePhase.enum.ready
      }
    });
  }

  // Still over the line: note the step finishing, or the user taking over.
  function hold({ id, state, window, readyAt, continueAt }: {
    id: string;
    state: PaceState;
    window: UsageWindow;
    readyAt: number;
    continueAt: number;
  }) {
    const isWorkingNow = isWorking(id);
    let phase = state.phase;
    if (phase === PacePhase.enum.pausing && !isWorkingNow) {
      phase = PacePhase.enum.paused;
    } else if (phase !== PacePhase.enum.pausing && isWorkingNow) {
      phase = PacePhase.enum.overridden;
    }

    setPace({
      id,
      state: {
        ...state,
        phase,
        windowKind: window.kind,
        utilization: window.utilization,
        readyAt,
        continueAt
      }
    });
  }

  // A working session not yet paced: pause it if it is ahead of the line.
  async function paceWorking({ session, account }: {
    session: AgentSession;
    account: AccountUsage | null;
  }) {
    const binding = assessPace({
      account,
      now: Date.now(),
      isPaused: false
    });
    const shouldPause = binding !== null && isWorking(session.id);
    if (shouldPause) {
      await pause({
        session,
        ...binding
      });
    }
  }

  // As its window is about to reset a paused session continues on its own -
  // unless the user took it over, which ends the pause.
  async function continueAtReset({ id, state }: {
    id: string;
    state: PaceState;
  }) {
    if (state.phase === PacePhase.enum.overridden) {
      forgetPace(id);
      return;
    }

    await resume(id);
  }

  // A paced session: continue it at the reset, or move it along as the line
  // catches up. An unreadable account leaves it as it is.
  async function advancePaced({ id, state, account }: {
    id: string;
    state: PaceState;
    account: AccountUsage | null;
  }) {
    const now = Date.now();
    if (now >= state.continueAt) {
      await continueAtReset({
        id,
        state
      });
      return;
    }

    if (account === null) {
      return;
    }

    const binding = assessPace({
      account,
      now,
      isPaused: true
    });
    if (binding === null) {
      await release({
        id,
        state
      });
      return;
    }

    hold({
      id,
      state,
      ...binding
    });
  }

  async function paceSession({ session, account }: {
    session: AgentSession;
    account: AccountUsage | null;
  }) {
    const id = session.id;
    const state = paced.get(id);
    const isOwnedElsewhere = hasUsageLimit(id) || host.isHandingOff(id);
    if (isOwnedElsewhere) {
      forgetPace(id);
      return;
    }

    if (state === undefined) {
      await paceWorking({
        session,
        account
      });
      return;
    }

    await advancePaced({
      id,
      state,
      account
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
        forgetPace(id);
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

    if (host.mode() === PacingMode.enum.off) {
      for (const id of [...paced.keys()]) {
        forgetPace(id);
      }
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
    /** The first session waiting on PADE or the user to continue - its id and
     *  status line - or null when none is. */
    get pause() {
      for (const session of host.sessions()) {
        const state = paced.get(session.id);
        const isWaiting = state !== undefined && state.phase !== PacePhase.enum.overridden;
        if (!isWaiting) {
          continue;
        }

        const isReady = state.phase === PacePhase.enum.ready;
        const isContinuingWhenReady = host.mode() === PacingMode.enum.continue || state.isContinuingWhenReady;
        return {
          id: session.id,
          isReady,
          /** Whether the user can still ask for it to continue once it has room. */
          canContinueWhenReady: !isReady && !isContinuingWhenReady,
          note: pacingNote({
            agent: session.agent.label,
            state,
            isContinuingWhenReady,
            now: Date.now()
          })
        };
      }

      return null;
    },
    /** The user picks a paused session back up now. */
    continueNow: resume,
    /** The user has a paused session continue the moment it has room. */
    continueWhenReady(id: string) {
      const state = paced.get(id);
      if (state === undefined) {
        return;
      }

      setPace({
        id,
        state: {
          ...state,
          isContinuingWhenReady: true
        }
      });
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
