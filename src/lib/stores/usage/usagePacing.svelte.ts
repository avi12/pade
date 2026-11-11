// Usage pacing: an agent on track to run out of its usage window well before the
// window resets is held back, so the account doesn't sit locked out for hours at
// the end of it. The formula - how far ahead of pace each window spends, the
// session by its burn, the week against an even spend - is lib/usage/pacing/usage-pace;
// the one switch and the built-in way of pacing are lib/usage/pacing/pacing-policy.
// This module measures each account's burn rate from its readings while its
// agents work, and climbs the steps one at a time while an agent stays too
// fast, each agent against its own account:
//
//   effort - one effort level down (lib/stores/usage/usageAdapt), repeated to the
//            bottom of its ladder before the next step;
//   model  - a cheaper model;
//   rest   - a short rest, sized so stretches of work and rest average out to a
//            pace that lasts, then "continue" - while that needs no more rest
//            than work;
//   stop   - stop at its next safe point until its runway reaches the reset,
//            then continue on its own.
//
// A stop or rest costs no turn when the agent is already idle: the pause
// request is typed only into running work, and "continue" only into work that
// pacing stopped.
//
// Every step undoes itself as room comes back, and all of it reverts as the
// window is about to reset. The user can override any agent from its tab badge
// (`easePacing`, `skipPacing`, `resumePacing`): ease one step, leave it alone for
// the rest of its window, or resume a stopped one now. A user who restarts a
// paused agent by hand wins the same way. Pauses are remembered per window, so
// a reload keeps them. The app shell supplies sessions and settings through
// `PacingHost` and drives the scan from a component `$effect`.
//
// An account's slowdown is one decision over every live session in every
// window and each subagent they run (lib/stores/usage/usageAccountRound): this
// window reports its working sessions, the round's leader decides, and this
// window takes the steps that fall to its own sessions. Running subagents are
// held, wrapped up and kept from spawning through the session's pacing control
// (lib/stores/usage/pacingControl), which the agent's hook reads on every tool
// call; a stop is written there too, so it lands mid-turn, not only at its end.

import { hasStopAllAction } from "@/lib/agent/interrupt-keys";
import { submitPastedPrompt } from "@/lib/agent/submit-prompt";
import { windowLabel } from "@/lib/ipc/platform";
import { cheaperModelChoice, lowerEffortChoice } from "@/lib/model/model-cost";
import { registerSessionState, SessionStateKey } from "@/lib/session/lifecycle/session-scope";
import { agentReportsBusy, backgroundAgentCount, isTurnRunningOnScreen } from "@/lib/stores/agent/agentActivity.svelte";
import { sessionModel } from "@/lib/stores/agent/context.svelte";
import { modelOptions } from "@/lib/stores/model/modelOptions.svelte";
import { isModelSwitchApplying } from "@/lib/stores/model/modelSwitch.svelte";
import { sessionLabel } from "@/lib/stores/session/sessionLabels.svelte";
import { sessionStatus } from "@/lib/stores/session/sessions.svelte";
import { setStopPart } from "@/lib/stores/usage/pacingControl.svelte";
import type { ControlTarget } from "@/lib/stores/usage/pacingControl.svelte";
import { forgetTerminalStop, stopThroughTerminal } from "@/lib/stores/usage/pacingInterrupt.svelte";
import { createAccountRound } from "@/lib/stores/usage/usageAccountRound.svelte";
import { adaptedChoice, adaptedSteps, createUsageAdapter } from "@/lib/stores/usage/usageAdapt.svelte";
import type { AdaptHost } from "@/lib/stores/usage/usageAdapt.svelte";
import { hasUsageLimit, readAccount } from "@/lib/stores/usage/usageResume.svelte";
import { PacedWindowKind, PacingStep, SessionStatus } from "@/lib/types";
import type { AccountUsage, AgentSession, UsageWindow } from "@/lib/types";
import type { RoundPlan, SessionReport } from "@/lib/usage/pacing/account-round";
import {
  costWeight,
  lightestToRestore,
  measuredPerEstimated,
  sessionLoad,
  shedFraction
} from "@/lib/usage/pacing/load-shedding";
import { measuredBurn } from "@/lib/usage/pacing/measured-burn";
import type { MeasuredBurn } from "@/lib/usage/pacing/measured-burn";
import { MILLISECONDS_PER_MINUTE, PACING_STEP_ORDER, pacingMargin, spokenTime } from "@/lib/usage/pacing/pacing-policy";
import {
  assessPace,
  burnRate,
  hasRoomToSpare,
  isRestEnough,
  MINIMUM_RATE_SPAN_MS,
  paceForecasts,
  pauseRequest,
  RATE_HISTORY_MS,
  resumeRequest,
  slowRestUntil,
  USER_CONTINUE_REQUEST,
  windowName
} from "@/lib/usage/pacing/usage-pace";
import type { PaceForecast, UsageSample } from "@/lib/usage/pacing/usage-pace";
import { afterDelay } from "@/lib/util/async/delay";
import { readStoredJson, writeStoredJson } from "@/lib/util/data/stored-json";
import { formatPercent } from "@/lib/util/format";
import { SvelteMap } from "svelte/reactivity";
import { z } from "zod";

/** How often working and paused sessions are checked against their account. The
 *  backend caches that read for ~3 minutes, so the endpoint is never hit faster. */
const POLL_MS = 60_000;
/** How far one scan moves a session's working share toward whether it works
 *  now - a moving average spanning about the burn rate's own history. */
const WORKING_SHARE_SMOOTHING = POLL_MS / RATE_HISTORY_MS;
/** How long an account waits after letting one paused session go before the
 *  next - long enough for its usage to show what the first one spends. */
const RELEASE_SPACING_MS = MINIMUM_RATE_SPAN_MS;
const MINUTES = new Intl.NumberFormat(undefined, {
  style: "unit",
  unit: "minute",
  unitDisplay: "long"
});
/** How many of a session's pacing actions its badge keeps. */
const HISTORY_LENGTH = 6;
/** How long "Skip this window" holds when the window's reset isn't known. */
const SKIP_FALLBACK_MS = 60 * MILLISECONDS_PER_MINUTE;
/** The steps that change what the agent runs rather than when it runs. */
const MODEL_STEPS: ReadonlySet<PacingStep> = new Set([PacingStep.enum.effort, PacingStep.enum.model]);

type PaceBinding = NonNullable<ReturnType<typeof assessPace>>;

/** Whether a rest is the step to take: the rest step, while resting needs no
 *  more rest than work - past that, a stop takes over. */
function shouldRest({ step, binding }: {
  step: PacingStep;
  binding: PaceBinding;
}) {
  return step === PacingStep.enum.rest && isRestEnough({
    ...binding,
    now: Date.now()
  });
}

/** How a session's history names an effort or model step. */
const STEP_HISTORY_TEXT: Partial<Record<PacingStep, string>> = {
  [PacingStep.enum.effort]: "effort turned down",
  [PacingStep.enum.model]: "moved to a cheaper model"
};

/** Where a paced session is: asked to stop and still finishing its step,
 *  stopped and waiting for room, or restarted by the user while over pace. */
const PacePhase = z.enum(["pausing", "paused", "overridden"]);

/** One paused session, as remembered across a reload of its window. */
const PaceState = z.object({
  phase: PacePhase,
  windowKind: PacedWindowKind,
  utilization: z.number(),
  /** When PADE asked the session to stop. */
  pausedAt: z.number(),
  /** When its runway reaches the reset again. */
  readyAt: z.number(),
  /** When its window is about to reset and it continues on its own. */
  continueAt: z.number(),
  /** Pacing stopped work the agent was running, so it is told to continue
   *  afterwards. An agent that was already idle is only held - a "continue"
   *  would start a turn nobody asked for. */
  hasStoppedWork: z.boolean().default(true),
  /** Each holding window's burn rate when it last held the session, by window
   *  key - a paused session burns nothing, so its readiness is judged against
   *  the pace it was working at. */
  rates: z.record(z.string(), z.number()).default({}),
  /** A resting session's rest ends here and it continues on its own; null for
   *  a stopped one. Planned when PADE asks it to stop, and set again from
   *  `restMs` once it has - its step can outlast the whole rest. */
  restUntil: z.number().nullable().default(null),
  /** How long a resting session rests, counted from when it actually stops. */
  restMs: z.number().nullable().default(null),
  /** Whether PADE has sent the pause request. A running turn is stopped with Esc
   *  instead - no extra turn spent re-reading the context - and the request
   *  follows only if delegated work keeps going after it. */
  hasSentPauseRequest: z.boolean().default(true)
});
type PaceState = z.infer<typeof PaceState>;

const PacedSessions = z.record(z.string(), PaceState);

/** Keyed by window label, like the session snapshot, so a window rebuilt after
 *  a crash finds the pauses of the sessions it re-attaches. */
function pacingStorageKey() {
  return `pade.usage-pacing:${windowLabel()}`;
}

const paced = new SvelteMap<string, PaceState>();
/** Why each held session is held: the window that holds it, and its numbers. */
const verdicts = new SvelteMap<string, {
  windowKind: PacedWindowKind;
  utilization: number;
  runOutAt: number;
  readyAt: number;
  resetAt: number;
}>();
/** What pacing did to each session, newest last. */
const histories = new SvelteMap<string, {
  at: number;
  text: string;
}[]>();
/** Sessions the user told pacing to leave alone, and until when. */
const skippedUntil = new SvelteMap<string, number>();
/** Sessions the user continued anyway, left alone until that run ends - true
 *  once the run is underway, so a scan before the agent picks the prompt up
 *  doesn't end it early. */
// eslint-disable-next-line svelte/prefer-svelte-reactivity -- written per scan, never rendered
const continuedAnyway = new Map<string, boolean>();
/** Each agent's account forecast per window, for the Config panel's live lines. */
const forecasts = new SvelteMap<string, Partial<Record<PacedWindowKind, PaceForecast>>>();
/** The live loop's sessions, for the count of those pacing slows. */
let sessionIdsSource: (() => string[]) | null = null;
/** When each working session's current stretch of work began - how long a
 *  resting session has just worked, which sizes its rest. */
// eslint-disable-next-line svelte/prefer-svelte-reactivity -- written per scan, never rendered
const workingSince = new Map<string, number>();
/** How much of the recent time each session spent working, 0 to 1 - one input
 *  to its share of its account's spend (`loadOf`). */
// eslint-disable-next-line svelte/prefer-svelte-reactivity -- written per scan, never rendered
const workingShares = new Map<string, number>();
/** The account each session bills, as last read - so an override can start its
 *  burn rate over. */
// eslint-disable-next-line svelte/prefer-svelte-reactivity -- written per scan, never rendered
const accountKeys = new Map<string, string>();
/** Recent working readings per account window, for its burn rate. An account is
 *  shared by every agent billing it, so their work adds up in one history. */
// eslint-disable-next-line svelte/prefer-svelte-reactivity -- written per scan, never rendered
const samples = new Map<string, UsageSample[]>();

/** The readings, kept across a reload of the window like its pauses - a burn
 *  rate takes ten minutes of work to measure, and a reload must not restart it. */
const StoredSamples = z.record(
  z.string(), z.array(
    z.object({
      at: z.number(),
      utilization: z.number(),
      resetsAt: z.string().nullish()
    })
  )
);

function samplesStorageKey() {
  return `pade.usage-pacing-samples:${windowLabel()}`;
}

function rememberSamples() {
  writeStoredJson({
    key: samplesStorageKey(),
    value: Object.fromEntries(samples)
  });
}

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

function record({ id, text }: {
  id: string;
  text: string;
}) {
  const history = [...(histories.get(id) ?? []), {
    at: Date.now(),
    text
  }];
  histories.set(id, history.slice(-HISTORY_LENGTH));
}

registerSessionState(SessionStateKey.enum.usagePacing, {
  forget(id) {
    forgetPace(id);
    forgetTerminalStop(id);
    verdicts.delete(id);
    histories.delete(id);
    skippedUntil.delete(id);
    continuedAnyway.delete(id);
    workingSince.delete(id);
    workingShares.delete(id);
    accountKeys.delete(id);
  }
});

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

  rememberSamples();
}

/** Start an account's burn rate over - after a step or an override, the old
 *  pace no longer says anything about the new one. */
function forgetSamples(accountKey: string | undefined) {
  if (accountKey === undefined) {
    return;
  }

  for (const key of samples.keys()) {
    if (key.startsWith(`${accountKey}:`)) {
      samples.delete(key);
    }
  }

  rememberSamples();
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

/** Asked to rest but still finishing its step - its rest hasn't begun, so it
 *  has no end yet. */
/** Whether pacing's stop must hold mid-turn: the session was asked to stop or
 *  rest and is still finishing - every tool call of it and its subagents is
 *  refused until it has. Stopped, it is idle, and a turn it starts after is the
 *  user's own; one the user took over is theirs. */
export function isStoppingMidTurn(state: Pick<PaceState, "phase"> | undefined) {
  return state?.phase === PacePhase.enum.pausing;
}

function isRestPending(state: Pick<PaceState, "phase" | "restMs">) {
  return state.phase === PacePhase.enum.pausing && state.restMs !== null;
}

/** Where a held session moves: one that just stopped starts its rest now - not
 *  when PADE asked, since the step it finished may have outlasted the whole
 *  rest - and one the user restarted is taken over. */
function heldPhase({ state, isWorkingNow }: {
  state: Pick<PaceState, "phase" | "restUntil" | "restMs">;
  isWorkingNow: boolean;
}) {
  const unchanged = {
    phase: state.phase,
    restUntil: state.restUntil
  };
  const isPausing = state.phase === PacePhase.enum.pausing;
  if (isPausing && isWorkingNow) {
    return unchanged;
  }

  if (isPausing) {
    return {
      phase: PacePhase.enum.paused,
      restUntil: state.restMs === null ? state.restUntil : Date.now() + state.restMs
    };
  }

  if (isWorkingNow) {
    return {
      ...unchanged,
      phase: PacePhase.enum.overridden
    };
  }

  return unchanged;
}

/** When a paused session continues on its own - its rest's end, or the moment
 *  it has room - or null while it is still finishing its step and its rest
 *  hasn't begun. */
function continueTime(state: Pick<PaceState, "phase" | "readyAt" | "restUntil" | "restMs">) {
  if (isRestPending(state)) {
    return null;
  }

  return state.restUntil ?? state.readyAt;
}

/** The status line for a stopped or resting session. */
export function pacingNote({ agent, state, now }: {
  agent: string;
  state: Pick<PaceState, "phase" | "windowKind" | "utilization" | "pausedAt" | "readyAt" | "restUntil" | "restMs">;
  now: number;
}) {
  const usage = `${windowName(state.windowKind)} usage (${formatPercent(state.utilization)})`;
  if (state.restUntil !== null) {
    return restNote({
      agent,
      state,
      usage,
      now
    });
  }

  return stopNote({
    agent,
    state,
    usage,
    now
  });
}

function restNote({ agent, state, usage, now }: {
  agent: string;
  state: Pick<PaceState, "phase" | "restUntil" | "restMs">;
  usage: string;
  now: number;
}) {
  if (isRestPending(state)) {
    const restMs = state.restMs ?? 0;
    const minutes = Math.max(1, Math.round(restMs / MILLISECONDS_PER_MINUTE));
    const earliestEnd = spokenTime({
      time: now + restMs,
      now
    });
    return `${agent} is finishing its step, then rests for ${MINUTES.format(minutes)} (until ${earliestEnd} at the earliest) to slow its ${usage}`;
  }

  return `${agent} is resting until ${spokenTime({
    time: state.restUntil ?? now,
    now
  })} to slow its ${usage}`;
}

function stopNote({ agent, state, usage, now }: {
  agent: string;
  state: Pick<PaceState, "pausedAt" | "readyAt">;
  usage: string;
  now: number;
}) {
  const pausedAt = spokenTime({
    time: state.pausedAt,
    now
  });
  const continuesAt = spokenTime({
    time: state.readyAt,
    now
  });
  return `${agent} paused at ${pausedAt} to pace its ${usage} - continues on its own around ${continuesAt}`;
}

/** The steps pacing has active on a session right now, in climbing order. */
function activeSteps(id: string) {
  const taken: PacingStep[] = adaptedSteps(id);
  const state = paced.get(id);
  if (state !== undefined && state.phase !== PacePhase.enum.overridden) {
    taken.push(state.restUntil === null ? PacingStep.enum.stop : PacingStep.enum.rest);
  }

  return PACING_STEP_ORDER.filter(step => taken.includes(step));
}

/** How many of this window's sessions pacing is slowing right now (reactive),
 *  for the Config panel's live line. */
export function slowedSessionCount() {
  let count = 0;
  for (const id of sessionIdsSource?.() ?? []) {
    if (activeSteps(id).length > 0) {
      count += 1;
    }
  }

  return count;
}

/** Everything a session's pacing badge shows (reactive): which steps are
 *  active, why, what it runs now, when it reverts, and what pacing did. Null
 *  when pacing isn't touching the session. */
export function pacingStatus(id: string) {
  const steps = activeSteps(id);
  if (steps.length === 0) {
    return null;
  }

  const state = paced.get(id);
  const hasRestEnd = state !== undefined && !isRestPending(state);
  return {
    steps,
    verdict: verdicts.get(id) ?? null,
    isStopped: state !== undefined,
    /** When a paused session continues on its own. */
    continuesAt: state === undefined ? null : continueTime(state),
    restUntil: hasRestEnd ? state.restUntil : null,
    revertsAt: state?.continueAt ?? null,
    history: histories.get(id) ?? []
  };
}

/** One window's forecast for an agent's account, for the Config panel - null
 *  until pacing has read that account (reactive). */
export function agentForecast({ agentId, kind }: {
  agentId: string;
  kind: PacedWindowKind;
}) {
  return forecasts.get(agentId)?.[kind] ?? null;
}

/** The overrides a badge offers, wired to the live pacing loop. */
let controller: {
  ease: (id: string) => Promise<void>;
  skip: (id: string) => Promise<void>;
  resume: (id: string) => Promise<void>;
} | null = null;

/** Undo pacing's latest step on a session. */
export async function easePacing(id: string) {
  await controller?.ease(id);
}

/** Leave a session alone for the rest of its window, undoing every step. */
export async function skipPacing(id: string) {
  await controller?.skip(id);
}

/** Continue a stopped or resting session now. */
export async function resumePacing(id: string) {
  await controller?.resume(id);
}

/** What the app shell provides: the switch, read per scan so turning it off
 *  applies without a restart, and the model switch stepping rides on. */
export interface PacingHost extends AdaptHost {
  sessions: () => AgentSession[];
  /** Whether pacing runs (prefs.usagePacing). */
  isOn: () => boolean;
  /** Whether the auto-handoff flow owns the session right now. */
  isHandingOff: (id: string) => boolean;
}

/** The pacing loop, scoped to one app shell. The shell calls `check()` from a
 *  `$effect` and `dispose()` on destroy, and shows `pauses` while sessions wait. */
export function createUsagePacing(host: PacingHost) {
  let timer: ReturnType<typeof setTimeout> | undefined;
  let isDisposed = false;
  const adapter = createUsageAdapter(host);
  // An account's slowdown is one decision over every window's sessions and
  // their subagents: this window reports its own each scan, and takes the steps
  // the round's leader hands back to it (`takePlan`).
  const round = createAccountRound({
    pollMs: POLL_MS,
    onPlan: takePlan
  });
  /** This window's latest judgement of each working session, for the steps a
   *  round hands back after the scan that judged it. */
  // eslint-disable-next-line svelte/prefer-svelte-reactivity -- read per round, never rendered
  const assessed = new Map<string, WorkingAssessment>();
  /** Each measured session's running subagents this scan, each with its load. */
  // eslint-disable-next-line svelte/prefer-svelte-reactivity -- one scan's reads, never rendered
  const runningSubagents = new Map<string, MeasuredBurn["subagents"]>();
  /** Each measured session's own share this scan, its running subagents'
   *  excluded - they are candidates of their own. */
  // eslint-disable-next-line svelte/prefer-svelte-reactivity -- one scan's reads, never rendered
  const ownLoads = new Map<string, number>();
  /** The sessions whose control says they are being stopped. */
  // eslint-disable-next-line svelte/prefer-svelte-reactivity -- written per change, never rendered
  const stopping = new Map<string, ControlTarget>();
  /** Paused sessions a scan found back on pace, by account - only one of each
   *  account's is let go (`releaseOnePerAccount`). */
  // eslint-disable-next-line svelte/prefer-svelte-reactivity -- one scan's bookkeeping, never rendered
  const releasable = new Map<string, AgentSession[]>();
  /** When each account last let a paused session go. */
  // eslint-disable-next-line svelte/prefer-svelte-reactivity -- read per scan, never rendered
  const lastReleaseAt = new Map<string, number>();
  /** Each working session's measured burn this scan, in priced tokens per
   *  minute (`measuredBurn`) - read once per scan, before the accounts are
   *  shared out. A session missing here has its load estimated. */
  // eslint-disable-next-line svelte/prefer-svelte-reactivity -- one scan's reads, never rendered
  const measuredLoads = new Map<string, number>();
  /** What one unit of estimated load measures as this scan
   *  (`measuredPerEstimated`). */
  let estimateScale = 1;

  for (const [id, state] of Object.entries(
    readStoredJson({
      key: pacingStorageKey(),
      schema: PacedSessions
    }) ?? {}
  )) {
    paced.set(id, state);
  }

  for (const [key, readings] of Object.entries(
    readStoredJson({
      key: samplesStorageKey(),
      schema: StoredSamples
    }) ?? {}
  )) {
    samples.set(key, readings);
  }

  function sessionOf(id: string) {
    return host.sessions().find(session => session.id === id);
  }

  sessionIdsSource = () => host.sessions().map(session => session.id);

  // A turn running, by the agent's own report or, for an agent that reports
  // nothing, by its output.
  function isWorking(id: string) {
    return agentReportsBusy(id) || sessionStatus(id) === SessionStatus.enum.working;
  }

  // Anything a pause would have to stop: its turn, or work it delegated.
  function isRunningWork(id: string) {
    return isWorking(id) || backgroundAgentCount(id) > 0;
  }

  async function pause({ session, kind, window, rate, readyAt, continueAt, isResting }: {
    session: AgentSession;
    kind: PacedWindowKind;
    window: UsageWindow;
    rate: number;
    readyAt: number;
    continueAt: number;
    isResting: boolean;
  }) {
    const now = Date.now();
    const restUntil = isResting ? slowRestUntil({
      readyAt,
      continueAt,
      stretch: now - (workingSince.get(session.id) ?? now - MINIMUM_RATE_SPAN_MS),
      now
    }) : null;
    // Already idle - its turn ended since it was judged - so it is simply held:
    // nothing to stop, and no request typed in, which would only spend a turn.
    const isIdle = !isRunningWork(session.id);
    const isInterrupting = isTurnRunningOnScreen(session.id);
    const isStoppedByKeys = isInterrupting || hasStopAllAction(session.agent.command);
    setPace({
      id: session.id,
      state: {
        phase: isIdle ? PacePhase.enum.paused : PacePhase.enum.pausing,
        windowKind: kind,
        utilization: window.utilization,
        pausedAt: now,
        readyAt,
        continueAt,
        hasStoppedWork: !isIdle,
        rates: {
          [window.key]: rate
        },
        restUntil,
        restMs: restUntil === null ? null : restUntil - now,
        hasSentPauseRequest: isIdle || !isStoppedByKeys
      }
    });
    record({
      id: session.id,
      text: restUntil === null ? "stopped" : `resting until ${spokenTime({
        time: restUntil,
        now
      })}`
    });
    await syncStops();

    if (isIdle) {
      return;
    }

    await stopRunningWork({
      session,
      window,
      stopWrittenAt: now,
      isStoppedByKeys,
      isResting
    });
  }

  // Stop the work a paused session is running: the hook takes the stop on its
  // next tool call, the terminal stop presses the agent's keys when it doesn't
  // (`pacingInterrupt`), and the pause request covers work no key can stop.
  async function stopRunningWork({ session, window, stopWrittenAt, isStoppedByKeys, isResting }: {
    session: AgentSession;
    window: UsageWindow;
    stopWrittenAt: number;
    isStoppedByKeys: boolean;
    isResting: boolean;
  }) {
    stopThroughTerminal({
      session,
      stopWrittenAt,
      isStillStopping: () => isStoppingMidTurn(paced.get(session.id)),
      isWorking: () => isWorking(session.id),
      hasBackgroundAgents: () => backgroundAgentCount(session.id) > 0
    });

    // Its work may have ended while the stop was written.
    if (isStoppedByKeys || !isRunningWork(session.id)) {
      return;
    }

    await askToStop({
      id: session.id,
      window,
      isSlowing: isResting
    });
  }

  // The pause request: one extra turn, spent only where no key can stop the
  // work - background agents, workflows and teammates outlive the turn Esc
  // ends, and only an agent with a stop-all action can end them by key.
  async function askToStop({ id, window, isSlowing }: {
    id: string;
    window: UsageWindow;
    isSlowing: boolean;
  }) {
    await submitPastedPrompt({
      id,
      text: pauseRequest({
        window,
        now: Date.now(),
        isSlowing
      }),
      wait: afterDelay
    });
  }

  // After Esc ended the turn, work still reported is delegated work: ask the
  // agent to stop it, once.
  async function stopDelegatedWork({ id, state, window }: {
    id: string;
    state: PaceState;
    window: UsageWindow;
  }) {
    const isDelegatedWorkRunning = state.phase === PacePhase.enum.pausing
      && !state.hasSentPauseRequest
      && (isWorking(id) || backgroundAgentCount(id) > 0)
      && !isTurnRunningOnScreen(id);
    if (!isDelegatedWorkRunning) {
      return state;
    }

    const asked = {
      ...state,
      hasSentPauseRequest: true
    };
    setPace({
      id,
      state: asked
    });
    await askToStop({
      id,
      window,
      isSlowing: state.restMs !== null
    });
    return asked;
  }

  async function resume(id: string) {
    await pickUp({
      id,
      request: resumeRequest(),
      isOnlyAfterStoppedWork: true
    });
  }

  // Pick a paused session back up - unless it is already working: a "continue"
  // typed into a running turn would queue a stray one behind it. Pacing's own
  // continue goes only to work it stopped; an agent that was idle stays so.
  async function pickUp({ id, request, isOnlyAfterStoppedWork }: {
    id: string;
    request: string;
    isOnlyAfterStoppedWork: boolean;
  }) {
    const state = paced.get(id);
    if (state === undefined) {
      return;
    }

    forgetPace(id);
    forgetTerminalStop(id);
    await syncStops();
    workingSince.set(id, Date.now());
    record({
      id,
      text: "continued"
    });

    const isNothingToContinue = isOnlyAfterStoppedWork && !state.hasStoppedWork;
    if (isWorking(id) || isNothingToContinue) {
      return;
    }

    await submitPastedPrompt({
      id,
      text: request,
      wait: afterDelay
    });
  }

  // The user's own continue: unlike pacing's, it owns the run it starts.
  async function continueAnyway(id: string) {
    continuedAnyway.set(id, false);
    await pickUp({
      id,
      request: USER_CONTINUE_REQUEST,
      isOnlyAfterStoppedWork: false
    });
  }

  // Back on pace. A session still finishing its step is left to stop first; one
  // the user took over is released; the rest continue, one per account at a time.
  async function release({ session, accountKey, state }: {
    session: AgentSession;
    accountKey: string;
    state: PaceState;
  }) {
    const id = session.id;
    const isStillStopping = state.phase === PacePhase.enum.pausing && isWorking(id);
    if (isStillStopping) {
      return;
    }

    if (state.phase === PacePhase.enum.overridden) {
      forgetPace(id);
      return;
    }

    releasable.set(accountKey, [...releasable.get(accountKey) ?? [], session]);
  }

  // The sessions back on pace continue one per account at a time, the lightest
  // first, each RELEASE_SPACING_MS after the last - so the account's usage shows
  // what that one spends before the next is let go, instead of every paused
  // session bursting back at once and tipping it straight over pace again.
  async function releaseOnePerAccount() {
    const now = Date.now();
    for (const [accountKey, sessions] of releasable) {
      const isSpaced = now - (lastReleaseAt.get(accountKey) ?? 0) >= RELEASE_SPACING_MS;
      const id = isSpaced ? lightestToRestore(
        sessions.map(session => ({
          id: session.id,
          load: loadOf(session)
        }))
      ) : null;
      if (id !== null) {
        lastReleaseAt.set(accountKey, now);
        await resume(id);
      }
    }

    releasable.clear();
  }

  // Still over pace: note the step finishing, or the user taking over.
  function hold({ id, state, kind, window, rate, readyAt, continueAt }: {
    id: string;
    state: PaceState;
    kind: PacedWindowKind;
    window: UsageWindow;
    rate: number;
    readyAt: number;
    continueAt: number;
  }) {
    setPace({
      id,
      state: {
        ...state,
        ...heldPhase({
          state,
          isWorkingNow: isWorking(id)
        }),
        windowKind: kind,
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

  // Take the first allowed step that still has somewhere to go. Effort and model
  // step down a notch at a time; resting gives way to a stop once it would need
  // more rest than work - or stays, when a stop isn't allowed.
  // An effort or model step, one notch down. Resolves whether it took one.
  async function stepModel({ session, binding, step }: {
    session: AgentSession;
    binding: PaceBinding;
    step: PacingStep;
  }) {
    const hasStepped = await adapter.stepDown({
      session,
      restoreAt: binding.continueAt,
      step
    });
    if (!hasStepped) {
      return false;
    }

    record({
      id: session.id,
      text: `${STEP_HISTORY_TEXT[step]} - now ${adaptedChoice(session.id)?.current ?? ""}`
    });
    return true;
  }

  // Take the first step that still has somewhere to go.
  async function climb({ session, binding }: {
    session: AgentSession;
    binding: PaceBinding;
  }) {
    for (const step of PACING_STEP_ORDER) {
      if (MODEL_STEPS.has(step)) {
        const hasStepped = await stepModel({
          session,
          binding,
          step
        });
        if (hasStepped) {
          return;
        }

        continue;
      }

      const isResting = shouldRest({
        step,
        binding
      });
      const isStopping = step === PacingStep.enum.stop;
      if (isResting || isStopping) {
        await pause({
          session,
          ...binding,
          isResting
        });
        return;
      }
    }
  }

  // A working session not yet paused, judged against its account:
  // over pace (its binding), or on pace with room to step a stepped-down choice
  // back up. What to do about it is the account's call (`reportOf`, the account-wide round).
  function assessWorking({ session, account, accountKey }: {
    session: AgentSession;
    account: AccountUsage | null;
    accountKey: string;
  }) {
    const now = Date.now();
    function rateOf(window: UsageWindow) {
      return liveRate({
        accountKey,
        window
      });
    }

    const binding = assessPace({
      account,
      now,
      isPaused: false,
      marginOf: pacingMargin,
      rateOf
    });
    if (binding === null) {
      verdicts.delete(session.id);
    } else {
      verdicts.set(session.id, {
        windowKind: binding.kind,
        utilization: binding.window.utilization,
        runOutAt: now + (binding.continueAt - binding.readyAt),
        readyAt: binding.readyAt,
        resetAt: binding.continueAt
      });
    }

    const hasRoom = binding === null && hasRoomToSpare({
      account,
      now,
      marginOf: pacingMargin,
      rateOf
    });
    return {
      session,
      accountKey,
      binding,
      hasRoom,
      canStepUp: hasRoom && adapter.isSteppedDown(session.id)
    };
  }

  type WorkingAssessment = ReturnType<typeof assessWorking>;

  // A session's share-driver of its account's spend: what it and its subagents
  // measurably burned lately, or - with nothing measured - its estimate, put on
  // the measured sessions' scale.
  function loadOf(session: AgentSession) {
    return measuredLoads.get(session.id) ?? estimatedLoad(session) * estimateScale;
  }

  // What a session is estimated to spend: what it runs, how much of the recent
  // time it worked, and the subagents it runs now.
  function estimatedLoad(session: AgentSession) {
    const running = sessionModel(session.id);
    return sessionLoad({
      cost: costWeight({
        model: running?.id,
        effort: running?.effort ?? undefined
      }),
      workingShare: workingShares.get(session.id) ?? 1,
      subagents: backgroundAgentCount(session.id)
    });
  }

  // What the session's next step would save, as a fraction of its own load -
  // the same first step `climb` takes: an effort or model step priced by its
  // cost weight, a rest or stop saving all of it while it lasts.
  function nextStepSaving(session: AgentSession) {
    const running = sessionModel(session.id);
    const options = modelOptions(session.agent.command);
    for (const step of PACING_STEP_ORDER) {
      if (!MODEL_STEPS.has(step)) {
        return 1;
      }

      if (running === null) {
        continue;
      }

      const current = {
        model: running.id,
        effort: running.effort ?? undefined
      };
      const next = step === PacingStep.enum.effort ? lowerEffortChoice({
        model: running.id,
        effort: running.effort ?? null,
        options
      }) : cheaperModelChoice({
        model: running.id,
        options
      });
      if (next !== null) {
        return 1 - costWeight(next) / costWeight(current);
      }
    }

    return 0;
  }

  // A working session as the account-wide round sees it: its own share of the
  // account (its running subagents apart, each with its own), and - over pace -
  // what the account must shed and what its next step would save.
  function reportOf(assessment: WorkingAssessment): SessionReport {
    const { session, binding } = assessment;
    return {
      id: session.id,
      name: sessionLabel(session.id) ?? session.agent.label,
      accountKey: assessment.accountKey,
      command: session.agent.command,
      conversationId: session.conversationId ?? null,
      reportedAt: Date.now(),
      load: ownLoads.get(session.id) ?? loadOf(session),
      overPace: binding === null ? null : {
        shed: largestShed([binding]),
        saving: nextStepSaving(session)
      },
      canStepUp: assessment.canStepUp,
      hasRoom: assessment.hasRoom,
      subagents: runningSubagents.get(session.id) ?? []
    };
  }

  // A session of this window the round stepped: its next allowed step, by the
  // window that held it when this window judged it. Another window's is not
  // this one's to take.
  async function climbAssessed(id: string) {
    const assessment = assessed.get(id);
    const binding = assessment?.binding ?? null;
    if (assessment === undefined || binding === null) {
      return;
    }

    await climb({
      session: assessment.session,
      binding
    });
  }

  async function restoreAssessed(id: string) {
    const assessment = assessed.get(id);
    if (assessment === undefined) {
      return;
    }

    if (await adapter.stepUp(assessment.session)) {
      record({
        id,
        text: "stepped back up"
      });
    }
  }

  // The round's say over this window's sessions: the ones it steps take their
  // next step, the ones it restores climb back, and every window starts the
  // burn rate of an account a step went to over - the old pace no longer says
  // anything about the new one.
  async function takePlan(plan: RoundPlan) {
    for (const id of plan.climb) {
      await climbAssessed(id);
    }

    for (const id of plan.restore) {
      await restoreAssessed(id);
    }

    assessed.clear();
    for (const accountKey of plan.steppedAccounts) {
      forgetSamples(accountKey);
    }
  }

  // Tell each session's agent whether pacing is stopping it, so the stop holds
  // mid-turn - for the session and every subagent it runs - until it has
  // stopped (`pausing`); a stopped session is idle, and a turn it starts after
  // is the user's own. A session gone since is let go too, so a resumed
  // conversation never starts out stopped.
  //
  // Every open session that isn't stopping is told so each time, not just the
  // ones this instance stopped: a stop written before the window reloaded is
  // in no one's memory, and was left blocking every tool call for good. The
  // writes skip an unchanged value, so this costs nothing once settled.
  async function syncStops() {
    // eslint-disable-next-line svelte/prefer-svelte-reactivity -- one sync's lookup, never rendered
    const isStopping = new Map<string, ControlTarget>();
    for (const session of host.sessions()) {
      const target = {
        command: session.agent.command,
        conversationId: session.conversationId
      };
      if (isStoppingMidTurn(paced.get(session.id))) {
        isStopping.set(session.id, target);
      } else {
        await setStopPart({
          target,
          isStopped: false
        });
      }
    }

    for (const [id, target] of isStopping) {
      stopping.set(id, target);
      await setStopPart({
        target,
        isStopped: true
      });
    }

    for (const [id, target] of stopping) {
      if (!isStopping.has(id)) {
        stopping.delete(id);
        await setStopPart({
          target,
          isStopped: false
        });
      }
    }
  }

  // What the account must shed: the most any of its holding windows asks for -
  // null while none of them has a measured burn.
  function largestShed(bindings: readonly PaceBinding[]) {
    const now = Date.now();
    let fraction: number | null = null;
    for (const binding of bindings) {
      const shed = shedFraction({
        window: binding.window,
        rate: binding.rate > 0 ? binding.rate : null,
        freeAt: binding.continueAt,
        now
      });
      const isLargestShed = shed !== null && (fraction === null || shed > fraction);
      if (isLargestShed) {
        fraction = shed;
      }
    }

    return fraction;
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

  // A paused session: continue it at the end of its rest or its window, or move
  // it along as its runway reaches the reset. An unreadable account leaves it be.
  async function advancePaced({ session, state, account, accountKey }: {
    session: AgentSession;
    state: PaceState;
    account: AccountUsage | null;
    accountKey: string;
  }) {
    const id = session.id;
    const now = Date.now();
    // A rest counts from when the session stopped (`hold`), so one still
    // stopping - or stopped since the last scan - hasn't begun resting yet.
    const isRestOver = !isRestPending(state) && state.restUntil !== null && now >= state.restUntil;
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
      marginOf: pacingMargin,
      rateOf: window => state.rates[window.key] ?? liveRate({
        accountKey,
        window
      })
    });
    if (binding === null) {
      await release({
        session,
        accountKey,
        state
      });
      return;
    }

    hold({
      id,
      state: await stopDelegatedWork({
        id,
        state,
        window: binding.window
      }),
      ...binding
    });
  }

  // A run the user continued anyway is theirs until the agent goes quiet again -
  // its background work included - so pacing doesn't pause it on the next scan.
  function isContinuedRun(id: string) {
    const isUnderway = continuedAnyway.get(id);
    if (isUnderway === undefined) {
      return false;
    }

    if (isWorking(id)) {
      continuedAnyway.set(id, true);
      return true;
    }

    if (isUnderway) {
      continuedAnyway.delete(id);
      return false;
    }

    return true;
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
      return null;
    }

    const isSkipped = (skippedUntil.get(id) ?? 0) > Date.now();
    if (isSkipped || isModelSwitchApplying(id) || isContinuedRun(id)) {
      return null;
    }

    const state = paced.get(id);
    if (state === undefined) {
      return isWorking(id) ? assessWorking({
        session,
        account,
        accountKey
      }) : null;
    }

    await advancePaced({
      session,
      state,
      account,
      accountKey
    });
    return null;
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
    forecasts.set(
      session.agent.id, paceForecasts({
        account,
        now,
        rateOf: window => liveRate({
          accountKey,
          window
        })
      })
    );
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
      const isWorkingNow = isWorking(session.id);
      if (!isWorkingNow) {
        workingSince.delete(session.id);
      } else if (!workingSince.has(session.id)) {
        workingSince.set(session.id, now);
      }

      const sample = isWorkingNow ? 1 : 0;
      const previous = workingShares.get(session.id) ?? sample;
      workingShares.set(session.id, previous + WORKING_SHARE_SMOOTHING * (sample - previous));
    }
  }

  // One session worth a look, against its account (read once per agent per
  // scan): paced, or stood down while pacing is off. A working one not yet
  // paused comes back as its assessment for its account.
  async function lookAt({ session, accounts, sampled, now }: {
    session: AgentSession;
    accounts: Map<string, Promise<AccountUsage | null>>;
    sampled: Set<string>;
    now: number;
  }) {
    if (!host.isOn()) {
      await standDown(session);
      return null;
    }

    let pending = accounts.get(session.agent.id);
    if (pending === undefined) {
      pending = readAccount(session);
      accounts.set(session.agent.id, pending);
    }

    const account = await pending;
    const accountKey = account?.account ?? session.agent.id;
    accountKeys.set(session.id, accountKey);
    sampleOnce({
      session,
      account,
      accountKey,
      sampled,
      now
    });
    return await paceSession({
      session,
      account,
      accountKey
    });
  }

  // Measure each paced working session's burn - all at once, off the agents'
  // transcripts - and rescale the estimates of the rest onto it. A session
  // nothing was measured for keeps its estimate (`measuredBurn` never rejects).
  async function measureLoads({ sessions, now }: {
    sessions: readonly AgentSession[];
    now: number;
  }) {
    const measurable = host.isOn() ? sessions.filter(session => isWorking(session.id)) : [];
    const readings = await Promise.all(
      measurable.map(async session => ({
        session,
        measured: await measuredBurn({
          session,
          now
        })
      }))
    );
    measuredLoads.clear();
    ownLoads.clear();
    runningSubagents.clear();
    const scales: {
      measured: number;
      estimated: number;
    }[] = [];
    for (const { session, measured } of readings) {
      if (measured === null) {
        continue;
      }

      measuredLoads.set(session.id, measured.total);
      ownLoads.set(session.id, measured.own);
      runningSubagents.set(session.id, measured.subagents);
      scales.push({
        measured: measured.total,
        estimated: estimatedLoad(session)
      });
    }

    estimateScale = measuredPerEstimated(scales);
  }

  // One look at every session worth a look - working ones that may need pacing,
  // paced ones that may continue - then this window's working sessions go to
  // the account-wide round, which every window's sessions share.
  async function scan() {
    const sessions = host.sessions();
    const now = Date.now();
    trackSessions({
      sessions,
      now
    });
    await measureLoads({
      sessions,
      now
    });
    await adapter.restore({
      sessions,
      now
    });

    // eslint-disable-next-line svelte/prefer-svelte-reactivity -- one scan's reads, never rendered
    const accounts = new Map<string, Promise<AccountUsage | null>>();
    // eslint-disable-next-line svelte/prefer-svelte-reactivity -- one scan's bookkeeping, never rendered
    const sampled = new Set<string>();
    assessed.clear();
    for (const session of sessions) {
      const isWorthALook = paced.has(session.id) || isWorking(session.id) || adapter.isSteppedDown(session.id);
      const assessment = isWorthALook ? await lookAt({
        session,
        accounts,
        sampled,
        now
      }) : null;
      if (assessment !== null) {
        assessed.set(session.id, assessment);
      }
    }

    await syncStops();
    await round.submit({
      label: windowLabel(),
      at: now,
      sessions: [...assessed.values()].map(assessment => reportOf(assessment))
    });
    await releaseOnePerAccount();
  }

  // Pacing turned off for the session's project: let it be if paused, and put
  // it back on its own choice if stepped down.
  async function standDown(session: AgentSession) {
    verdicts.delete(session.id);
    await resume(session.id);
    await adapter.restoreSession(session);
  }

  async function tick() {
    timer = undefined;

    if (isDisposed) {
      return;
    }

    await scan();
    schedule();
  }

  function schedule() {
    if (isDisposed || timer !== undefined) {
      return;
    }

    timer = setTimeout(tick, POLL_MS);
  }

  controller = {
    async ease(id) {
      const session = sessionOf(id);
      if (session === undefined) {
        return;
      }

      forgetSamples(accountKeys.get(id));

      if (paced.has(id)) {
        await continueAnyway(id);
        return;
      }

      if (await adapter.stepUp(session)) {
        record({
          id,
          text: "eased a step"
        });
      }
    },
    async skip(id) {
      const session = sessionOf(id);
      if (session === undefined) {
        return;
      }

      const until = paced.get(id)?.continueAt ?? verdicts.get(id)?.resetAt ?? Date.now() + SKIP_FALLBACK_MS;
      skippedUntil.set(id, until);
      verdicts.delete(id);
      await resume(id);
      await adapter.restoreSession(session);
      record({
        id,
        text: `left alone until ${spokenTime({
          time: until,
          now: Date.now()
        })}`
      });
    },
    resume: continueAnyway
  };

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

        pauses.push({
          id: session.id,
          note: pacingNote({
            agent: session.agent.label,
            state,
            now: Date.now()
          })
        });
      }

      return pauses;
    },
    /** The user picks a paused session back up now, and pacing lets that run be. */
    continueNow: continueAnyway,
    /** Start the poll; reads nothing reactive, so the `$effect` runs it once. */
    check: schedule,
    dispose() {
      isDisposed = true;
      round.dispose();
      controller = null;
      sessionIdsSource = null;
      clearTimeout(timer);
      timer = undefined;
    }
  };
}
