// Usage pacing: a working agent that would spend a usage window long before it
// resets is turned down, slowed or stopped, so the account doesn't sit locked
// out for hours at the end of a window. The formula - runway against time left,
// with no window length assumed - is lib/usage/usage-pace; this module measures
// each account's burn rate from its readings while its agents work, and acts on
// the verdict for every session, each agent against its own account.
//
// Over pace, an agent is first stepped down to a cheaper effort, then model, when
// prefs.pacingAdapt is on (lib/stores/usage/usageAdapt), and stepped back up once
// it has room to spare. Past the cheapest step - or with stepping off - the
// prefs.pacingAction applies: `stop` asks it to stop at its next safe point until
// its runway reaches the reset; `slow` gives it a short rest, sized so stretches
// of work and rest average out to a pace that lasts, then continues it. PADE
// remembers when it stopped each session - per window, across a reload. Once a
// stopped session is ready, prefs.usagePacing says what happens: `continue`
// picks it straight back up, `ask` leaves the call to the user - and, left
// alone, it continues on its own as the window is about to reset. A user who
// restarts a paused agent by hand wins: that session is not paused again until it
// is back on pace. `off` paces nothing. The app shell supplies sessions through
// `PacingHost`, drives the scan from a component `$effect`, and shows the pauses
// with their Continue buttons.

import { submitPastedPrompt } from "@/lib/agent/submit-prompt";
import { Command, sendMessage } from "@/lib/ipc/messaging";
import { windowLabel } from "@/lib/ipc/platform";
import { agentReportsBusy } from "@/lib/stores/agent/agentActivity.svelte";
import { isModelSwitchApplying } from "@/lib/stores/model/modelSwitch.svelte";
import { sessionStatus } from "@/lib/stores/session/sessions.svelte";
import { createUsageAdapter } from "@/lib/stores/usage/usageAdapt.svelte";
import type { AdaptHost } from "@/lib/stores/usage/usageAdapt.svelte";
import { CONTINUE_PROMPT, hasUsageLimit, readAccount } from "@/lib/stores/usage/usageResume.svelte";
import { PacingAction, PacingMode, SessionStatus, UsageWindowKind } from "@/lib/types";
import type { AccountUsage, AgentSession, UsageWindow } from "@/lib/types";
import {
  assessPace,
  burnRate,
  hasRoomToSpare,
  MINIMUM_RATE_SPAN_MS,
  pauseRequest,
  RATE_HISTORY_MS,
  slowRestUntil,
  windowName
} from "@/lib/usage/usage-pace";
import type { UsageSample } from "@/lib/usage/usage-pace";
import { afterDelay } from "@/lib/util/delay";
import { formatPercent } from "@/lib/util/format";
import { readStoredJson, writeStoredJson } from "@/lib/util/stored-json";
import { SvelteMap } from "svelte/reactivity";
import { z } from "zod";

/** How often working and paused sessions are checked against their account. The
 *  backend caches that read for ~3 minutes, so the endpoint is never hit faster. */
const POLL_MS = 60_000;

/** Where a paced session is: asked to stop and still finishing its step, stopped
 *  and waiting for room, stopped with room again (the user's call), or restarted
 *  by the user while over pace. */
const PacePhase = z.enum(["pausing", "paused", "ready", "overridden"]);

/** One paced session, as remembered across a reload of its window. */
const PaceState = z.object({
  phase: PacePhase,
  windowKind: UsageWindowKind,
  utilization: z.number(),
  /** When PADE asked the session to stop. */
  pausedAt: z.number(),
  /** When its runway reaches the reset again. */
  readyAt: z.number(),
  /** When its window is about to reset and it continues on its own. */
  continueAt: z.number(),
  /** The user chose to have this session continue the moment it has room,
   *  whatever the pacing mode says. */
  isContinuingWhenReady: z.boolean().default(false),
  /** Each holding window's burn rate when it last held the session, by window
   *  key - a paused session burns nothing, so its readiness is judged against
   *  the pace it was working at. */
  rates: z.record(z.string(), z.number()).default({}),
  /** A slowed session's rest ends here and it continues on its own; null for
   *  a stopped one. */
  restUntil: z.number().nullable().default(null)
});
type PaceState = z.infer<typeof PaceState>;

const PacedSessions = z.record(z.string(), PaceState);

/** Keyed by window label, like the session snapshot, so a window rebuilt after
 *  a crash finds the pauses of the sessions it re-attaches. */
function pacingStorageKey() {
  return `pade.usage-pacing:${windowLabel()}`;
}

const paced = new SvelteMap<string, PaceState>();
/** When each working session's current stretch of work began - how long a
 *  slowed session has just worked, which sizes its rest. */
// eslint-disable-next-line svelte/prefer-svelte-reactivity -- written per scan, never rendered
const workingSince = new Map<string, number>();
/** Recent working readings per account window, for its burn rate. An account is
 *  shared by every agent billing it, so their work adds up in one history. */
// eslint-disable-next-line svelte/prefer-svelte-reactivity -- written per scan, never rendered
const samples = new Map<string, UsageSample[]>();

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
  workingSince.delete(id);
}

function sampleKey({ accountKey, window }: {
  accountKey: string;
  window: UsageWindow;
}) {
  return `${accountKey}:${window.key}`;
}

function recordSamples({ accountKey, account, now }: {
  accountKey: string;
  account: AccountUsage;
  now: number;
}) {
  for (const window of account.windows) {
    const key = sampleKey({
      accountKey,
      window
    });
    const recent = (samples.get(key) ?? []).filter(sample => sample.at >= now - RATE_HISTORY_MS);
    recent.push({
      at: now,
      utilization: window.utilization,
      resetsAt: window.resetsAt
    });
    samples.set(key, recent);
  }
}

/** Start an account's burn rate over - after an effort or model change, the old
 *  pace no longer says anything about the new one. */
function forgetSamples(accountKey: string) {
  for (const key of [...samples.keys()]) {
    if (key.startsWith(`${accountKey}:`)) {
      samples.delete(key);
    }
  }
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
  state: Pick<PaceState, "phase" | "windowKind" | "utilization" | "pausedAt" | "readyAt" | "continueAt" | "restUntil">;
  /** Whether it continues as soon as it has room, not just at the reset. */
  isContinuingWhenReady: boolean;
  now: number;
}) {
  const usage = `${windowName(state.windowKind)} usage (${formatPercent(state.utilization)})`;
  if (state.restUntil !== null) {
    return `${agent} is resting until ${spokenTime({
      time: state.restUntil,
      now
    })} to slow its ${usage}`;
  }

  const pausedAt = spokenTime({
    time: state.pausedAt,
    now
  });
  const continuesAt = spokenTime({
    time: isContinuingWhenReady ? state.readyAt : state.continueAt,
    now
  });
  if (state.phase === PacePhase.enum.ready) {
    return `${agent}, paused at ${pausedAt}, has room in its ${usage} again - continues on its own at ${continuesAt}`;
  }

  return `${agent} paused at ${pausedAt} to pace its ${usage} - continues on its own around ${continuesAt}`;
}

/** What the app shell provides - the settings, read per scan so a Config change
 *  applies without a restart, and the model switch stepping rides on. */
export interface PacingHost extends AdaptHost {
  sessions: () => AgentSession[];
  /** prefs.usagePacing - what a ready session does, or `off`. */
  mode: () => PacingMode;
  /** prefs.pacingAction - stop or slow a session over pace. */
  action: () => PacingAction;
  /** prefs.pacingAdapt - step effort and model first. */
  isAdapting: () => boolean;
  /** Whether the auto-handoff flow owns the session right now. */
  isHandingOff: (id: string) => boolean;
}

/** The pacing loop, scoped to one app shell. The shell calls `check()` from a
 *  `$effect` and `dispose()` on destroy, shows `pauses` while sessions wait, and
 *  calls `continueNow` / `continueWhenReady` from their buttons. */
export function createUsagePacing(host: PacingHost) {
  let timer: ReturnType<typeof setTimeout> | undefined;
  let isDisposed = false;
  const adapter = createUsageAdapter(host);

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

  function liveRate({ accountKey, window }: {
    accountKey: string;
    window: UsageWindow;
  }) {
    return burnRate(
      samples.get(
        sampleKey({
          accountKey,
          window
        })
      ) ?? []
    );
  }

  async function pause({ session, window, rate, readyAt, continueAt }: {
    session: AgentSession;
    window: UsageWindow;
    rate: number;
    readyAt: number;
    continueAt: number;
  }) {
    const now = Date.now();
    const isSlowing = host.action() === PacingAction.enum.slow;
    const restUntil = isSlowing ? slowRestUntil({
      readyAt,
      continueAt,
      stretch: now - (workingSince.get(session.id) ?? now - MINIMUM_RATE_SPAN_MS),
      now
    }) : null;
    setPace({
      id: session.id,
      state: {
        phase: PacePhase.enum.pausing,
        windowKind: window.kind,
        utilization: window.utilization,
        pausedAt: now,
        readyAt,
        continueAt,
        isContinuingWhenReady: false,
        rates: {
          [window.key]: rate
        },
        restUntil
      }
    });
    await submitPastedPrompt({
      id: session.id,
      text: pauseRequest({
        window,
        now,
        isSlowing
      }),
      wait: afterDelay
    });
  }

  // Pick a paused session back up - unless it is already working: a "continue"
  // typed into a running turn would queue a stray one behind it.
  async function resume(id: string) {
    forgetPace(id);
    workingSince.set(id, Date.now());

    if (isWorking(id)) {
      return;
    }

    await sendMessage(Command.ptyWrite, {
      id,
      data: CONTINUE_PROMPT
    });
  }

  // Back on pace. A session still finishing its step is left to stop first; one
  // the user took over is released; a slowed one continues; a stopped one
  // continues at once or waits for the user, as the pacing mode says.
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

    const shouldContinue = host.mode() === PacingMode.enum.continue
      || state.isContinuingWhenReady
      || state.restUntil !== null;
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

  // Still over pace: note the step finishing, or the user taking over.
  function hold({ id, state, window, rate, readyAt, continueAt }: {
    id: string;
    state: PaceState;
    window: UsageWindow;
    rate: number;
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
        continueAt,
        rates: {
          ...state.rates,
          [window.key]: rate
        }
      }
    });
  }

  // A working session not yet paced. Over pace: a notch cheaper when stepping is
  // on and there is a cheaper notch, else stopped or slowed. On pace with room to
  // spare: a stepped-down session climbs a notch back. Either step starts its
  // account's burn rate over, so the next one waits on a fresh measure.
  async function paceWorking({ session, account, accountKey }: {
    session: AgentSession;
    account: AccountUsage | null;
    accountKey: string;
  }) {
    const now = Date.now();
    const rateOf = (window: UsageWindow) => liveRate({
      accountKey,
      window
    });
    const binding = assessPace({
      account,
      now,
      isPaused: false,
      rateOf
    });
    if (binding === null) {
      const canStepUp = host.isAdapting() && adapter.isSteppedDown(session.id) && hasRoomToSpare({
        account,
        now,
        rateOf
      });
      if (canStepUp) {
        forgetSamples(accountKey);
        await adapter.stepUp(session);
      }

      return;
    }

    const hasSteppedDown = host.isAdapting() && await adapter.stepDown({
      session,
      restoreAt: binding.continueAt
    });
    if (hasSteppedDown) {
      forgetSamples(accountKey);
      return;
    }

    await pause({
      session,
      ...binding
    });
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

  // A paced session: continue it at the end of its rest or its window, or move it
  // along as its runway reaches the reset. An unreadable account leaves it be.
  async function advancePaced({ id, state, account, accountKey }: {
    id: string;
    state: PaceState;
    account: AccountUsage | null;
    accountKey: string;
  }) {
    const now = Date.now();
    const isRestOver = state.restUntil !== null && now >= state.restUntil;
    const isStillStopping = state.phase === PacePhase.enum.pausing && isWorking(id);
    const isDue = now >= state.continueAt || (isRestOver && !isStillStopping);
    if (isDue) {
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
      isPaused: true,
      rateOf: window => state.rates[window.key] ?? liveRate({
        accountKey,
        window
      })
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

  async function paceSession({ session, account, accountKey }: {
    session: AgentSession;
    account: AccountUsage | null;
    accountKey: string;
  }) {
    const id = session.id;
    const isOwnedElsewhere = hasUsageLimit(id) || host.isHandingOff(id);
    if (isOwnedElsewhere) {
      forgetPace(id);
      return;
    }

    if (isModelSwitchApplying(id)) {
      return;
    }

    const state = paced.get(id);
    if (state === undefined) {
      if (isWorking(id)) {
        await paceWorking({
          session,
          account,
          accountKey
        });
      }

      return;
    }

    await advancePaced({
      id,
      state,
      account,
      accountKey
    });
  }

  // Take one reading per account per scan, and only while one of its sessions
  // works - the burn rate measures work, not idle time.
  function sampleOnce({ session, account, accountKey, sampled, now }: {
    session: AgentSession;
    account: AccountUsage | null;
    accountKey: string;
    sampled: Set<string>;
    now: number;
  }) {
    const shouldSample = account !== null && isWorking(session.id) && !sampled.has(accountKey);
    if (!shouldSample) {
      return;
    }

    sampled.add(accountKey);
    recordSamples({
      accountKey,
      account,
      now
    });
  }

  // Track each session's current stretch of work, and forget closed sessions.
  function trackSessions({ sessions, now }: {
    sessions: readonly AgentSession[];
    now: number;
  }) {
    // eslint-disable-next-line svelte/prefer-svelte-reactivity -- one scan's lookup, never rendered
    const open = new Set(sessions.map(session => session.id));
    for (const id of paced.keys()) {
      if (!open.has(id)) {
        forgetPace(id);
      }
    }

    for (const session of sessions) {
      if (!isWorking(session.id)) {
        workingSince.delete(session.id);
      } else if (!workingSince.has(session.id)) {
        workingSince.set(session.id, now);
      }
    }
  }

  // One look at every session worth a look - working ones that may need pacing,
  // paced ones that may continue - reading each agent's account once.
  async function scan() {
    const sessions = host.sessions();
    const now = Date.now();
    trackSessions({
      sessions,
      now
    });
    await adapter.restore({
      sessions,
      now,
      isEveryone: !host.isAdapting()
    });

    // eslint-disable-next-line svelte/prefer-svelte-reactivity -- one scan's reads, never rendered
    const accounts = new Map<string, Promise<AccountUsage | null>>();
    // eslint-disable-next-line svelte/prefer-svelte-reactivity -- one scan's bookkeeping, never rendered
    const sampled = new Set<string>();
    for (const session of sessions) {
      const isWorthALook = paced.has(session.id) || isWorking(session.id);
      if (!isWorthALook) {
        continue;
      }

      let pending = accounts.get(session.agent.id);
      if (pending === undefined) {
        pending = readAccount(session);
        accounts.set(session.agent.id, pending);
      }

      const account = await pending;
      const accountKey = account?.account ?? session.agent.id;
      sampleOnce({
        session,
        account,
        accountKey,
        sampled,
        now
      });
      await paceSession({
        session,
        account,
        accountKey
      });
    }
  }

  // Pacing turned off: let every paused session be, and put stepped-down ones
  // back on their own choice.
  async function standDown() {
    for (const id of [...paced.keys()]) {
      forgetPace(id);
    }

    await adapter.restore({
      sessions: host.sessions(),
      now: Date.now(),
      isEveryone: true
    });
  }

  async function tick() {
    timer = undefined;

    if (isDisposed) {
      return;
    }

    if (host.mode() === PacingMode.enum.off) {
      await standDown();
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
    /** Every session waiting on PADE or the user to continue - each agent paced
     *  against its own account - with its id and status line. */
    get pauses() {
      const pauses = [];
      for (const session of host.sessions()) {
        const state = paced.get(session.id);
        const isWaiting = state !== undefined && state.phase !== PacePhase.enum.overridden;
        if (!isWaiting) {
          continue;
        }

        const isReady = state.phase === PacePhase.enum.ready;
        const isContinuingWhenReady = host.mode() === PacingMode.enum.continue
          || state.isContinuingWhenReady
          || state.restUntil !== null;
        pauses.push({
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
        });
      }

      return pauses;
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
