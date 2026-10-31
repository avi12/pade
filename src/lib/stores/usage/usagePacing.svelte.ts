// Usage pacing: an agent on track to run out of its usage window well before the
// window resets is held back, so the account doesn't sit locked out for hours at
// the end of it. The formula - how far ahead of pace each window spends, the
// session by its burn, the week against an even spend - is lib/usage/pacing/usage-pace;
// the steps and presets are lib/usage/pacing/pacing-policy, and which settings apply to
// this window's workspace is lib/usage/pacing/pacing-scope, each paced window (session,
// weekly) with its own. This module measures each account's burn rate from
// its readings while its agents work, and climbs the user's allowed steps one at
// a time while an agent stays too fast, each agent against its own account:
//
//   effort - one effort level down (lib/stores/usage/usageAdapt), repeated to the
//            bottom of its ladder before the next step;
//   model  - a cheaper model;
//   rest   - a short rest, sized so stretches of work and rest average out to a
//            pace that lasts, then "continue" - while that needs no more rest
//            than work, or Stop isn't allowed;
//   stop   - stop at its next safe point until its runway reaches the reset,
//            then ask the user or continue, as `stopThen` says.
//
// Every step undoes itself as room comes back, and all of it reverts as the
// window is about to reset. The user can override any agent from its tab badge
// (`easePacing`, `skipPacing`, `resumePacing`): ease one step, leave it alone for
// the rest of its window, or resume a stopped one now. A user who restarts a
// paused agent by hand wins the same way. Pauses are remembered per window, so
// a reload keeps them. The app shell supplies sessions and settings through
// `PacingHost` and drives the scan from a component `$effect`.

import { submitPastedPrompt } from "@/lib/agent/submit-prompt";
import { Command, sendMessage } from "@/lib/ipc/messaging";
import { windowLabel } from "@/lib/ipc/platform";
import { cheaperModelChoice, lowerEffortChoice } from "@/lib/model/model-cost";
import { registerSessionState, SessionStateKey } from "@/lib/session/lifecycle/session-scope";
import { agentReportsBusy, backgroundAgentCount, isTurnRunningOnScreen } from "@/lib/stores/agent/agentActivity.svelte";
import { sessionModel } from "@/lib/stores/agent/context.svelte";
import { modelOptions } from "@/lib/stores/model/modelOptions.svelte";
import { isModelSwitchApplying } from "@/lib/stores/model/modelSwitch.svelte";
import { sessionStatus } from "@/lib/stores/session/sessions.svelte";
import { adaptedChoice, adaptedSteps, createUsageAdapter } from "@/lib/stores/usage/usageAdapt.svelte";
import type { AdaptHost } from "@/lib/stores/usage/usageAdapt.svelte";
import { hasUsageLimit, readAccount } from "@/lib/stores/usage/usageResume.svelte";
import { ESCAPE } from "@/lib/terminal/input/terminal-sequences";
import { PacedWindowKind, PacingStep, PacingStopThen, SessionStatus } from "@/lib/types";
import type { AccountUsage, AgentSession, PacingByWindow, UsageWindow } from "@/lib/types";
import {
  costWeight,
  lightestToRestore,
  measuredPerEstimated,
  planShedding,
  sessionLoad,
  shedFraction
} from "@/lib/usage/pacing/load-shedding";
import { measuredBurn } from "@/lib/usage/pacing/measured-burn";
import {
  allowedSteps,
  isPacingOn,
  MILLISECONDS_PER_MINUTE,
  PACING_STEP_ORDER,
  pacingMargins,
  spokenTime
} from "@/lib/usage/pacing/pacing-policy";
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
 *  more rest than work - or always, when a stop isn't allowed to take over. */
function shouldRest({ step, allowed, binding }: {
  step: PacingStep;
  allowed: readonly PacingStep[];
  binding: PaceBinding;
}) {
  if (step !== PacingStep.enum.rest) {
    return false;
  }

  return !allowed.includes(PacingStep.enum.stop) || isRestEnough({
    ...binding,
    now: Date.now()
  });
}

/** How a session's history names an effort or model step. */
const STEP_HISTORY_TEXT: Partial<Record<PacingStep, string>> = {
  [PacingStep.enum.effort]: "effort turned down",
  [PacingStep.enum.model]: "moved to a cheaper model"
};

/** Where a paced session is: asked to stop and still finishing its step, stopped
 *  and waiting for room, stopped with room again (the user's call), or restarted
 *  by the user while over pace. */
const PacePhase = z.enum(["pausing", "paused", "ready", "overridden"]);

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
  /** The user chose to have this session continue the moment it has room. */
  isContinuingWhenReady: z.boolean().default(false),
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
/** The live loop's settings, so a badge counts the steps its window allows. */
let pacingSource: ((id: string) => PacingByWindow | null) | null = null;
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

/** Whether a paused session continues once it has room, not just at its reset:
 *  its settings say so, the user asked for it, or it only rests. */
function isContinuingOnRoom({ state, pacing }: {
  state: Pick<PaceState, "windowKind" | "isContinuingWhenReady" | "restUntil">;
  pacing: PacingByWindow;
}) {
  return pacing[state.windowKind].stopThen === PacingStopThen.enum.continue
    || state.isContinuingWhenReady
    || state.restUntil !== null;
}

/** When a stopped session continues on its own. */
function stopEndTime({ state, isContinuingWhenReady }: {
  state: Pick<PaceState, "readyAt" | "continueAt">;
  isContinuingWhenReady: boolean;
}) {
  return isContinuingWhenReady ? state.readyAt : state.continueAt;
}

/** When a paused session continues on its own - its rest's end, or its stop's -
 *  or null while it is still finishing its step and its rest hasn't begun. */
function continueTime({ state, isContinuingWhenReady }: {
  state: Pick<PaceState, "phase" | "readyAt" | "continueAt" | "restUntil" | "restMs">;
  isContinuingWhenReady: boolean;
}) {
  if (isRestPending(state)) {
    return null;
  }

  return state.restUntil ?? stopEndTime({
    state,
    isContinuingWhenReady
  });
}

/** The status line for a stopped or resting session. */
export function pacingNote({ agent, state, isContinuingWhenReady, now }: {
  agent: string;
  state: Pick<PaceState, "phase" | "windowKind" | "utilization" | "pausedAt" | "readyAt" | "continueAt" | "restUntil" | "restMs">;
  /** Whether it continues as soon as it has room, not just at the reset. */
  isContinuingWhenReady: boolean;
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
    isContinuingWhenReady,
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

function stopNote({ agent, state, usage, isContinuingWhenReady, now }: {
  agent: string;
  state: Pick<PaceState, "phase" | "pausedAt" | "readyAt" | "continueAt">;
  usage: string;
  isContinuingWhenReady: boolean;
  now: number;
}) {
  const pausedAt = spokenTime({
    time: state.pausedAt,
    now
  });
  const continuesAt = spokenTime({
    time: stopEndTime({
      state,
      isContinuingWhenReady
    }),
    now
  });
  if (state.phase === PacePhase.enum.ready) {
    return `${agent}, paused at ${pausedAt}, has room in its ${usage} again - continues on its own at ${continuesAt}`;
  }

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

/** How many steps the window holding a session allows it - the session window
 *  when nothing names one yet. */
function allowedStepCount(id: string) {
  const pacing = pacingSource?.(id) ?? null;
  if (pacing === null) {
    return 0;
  }

  const kind = paced.get(id)?.windowKind ?? verdicts.get(id)?.windowKind ?? PacedWindowKind.enum.session;
  return allowedSteps(pacing[kind]).length;
}

/** When a paused session continues on its own, by the same rule its pause note
 *  states - null when it isn't paused, or still finishes its step. */
function pausedUntil(id: string) {
  const state = paced.get(id);
  const pacing = pacingSource?.(id) ?? null;
  if (state === undefined || pacing === null) {
    return null;
  }

  return continueTime({
    state,
    isContinuingWhenReady: isContinuingOnRoom({
      state,
      pacing
    })
  });
}

/** Everything a session's pacing badge shows (reactive): which steps are active
 *  out of the allowed ones, why, what it runs now, when it reverts, and what
 *  pacing did. Null when pacing isn't touching the session. */
export function pacingStatus(id: string) {
  const steps = activeSteps(id);
  if (steps.length === 0) {
    return null;
  }

  const state = paced.get(id);
  const hasRestEnd = state !== undefined && !isRestPending(state);
  return {
    steps,
    allowedCount: allowedStepCount(id),
    verdict: verdicts.get(id) ?? null,
    isStopped: state !== undefined,
    /** When a paused session continues on its own. */
    continuesAt: pausedUntil(id),
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

/** What the app shell provides: the settings, read per scan so a Config change
 *  applies without a restart, and the model switch stepping rides on. */
export interface PacingHost extends AdaptHost {
  sessions: () => AgentSession[];
  /** A project's settings, per paced window - each session is paced by its own
   *  project's, not by the one in front. */
  settings: (project: string) => PacingByWindow;
  /** Whether the auto-handoff flow owns the session right now. */
  isHandingOff: (id: string) => boolean;
}

/** The pacing loop, scoped to one app shell. The shell calls `check()` from a
 *  `$effect` and `dispose()` on destroy, and shows `pauses` while sessions wait. */
export function createUsagePacing(host: PacingHost) {
  let timer: ReturnType<typeof setTimeout> | undefined;
  let isDisposed = false;
  const adapter = createUsageAdapter(host);
  // Accounts a scan stepped a session of. Their burn rate starts over once the
  // scan ends, not at the first step: the rate is the whole account's - every
  // session on it, and every subagent those run - so each of its sessions over
  // pace takes its step on the same reading, instead of one per fresh measure.
  // eslint-disable-next-line svelte/prefer-svelte-reactivity -- one scan's bookkeeping, never rendered
  const steppedAccounts = new Set<string>();
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

  function settingsOf(session: AgentSession) {
    return host.settings(session.project);
  }

  pacingSource = id => {
    const session = sessionOf(id);
    return session === undefined ? null : settingsOf(session);
  };

  // A turn running, by the agent's own report or, for an agent that reports
  // nothing, by its output.
  function isWorking(id: string) {
    return agentReportsBusy(id) || sessionStatus(id) === SessionStatus.enum.working;
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
    const isInterrupting = isTurnRunningOnScreen(session.id);
    setPace({
      id: session.id,
      state: {
        phase: PacePhase.enum.pausing,
        windowKind: kind,
        utilization: window.utilization,
        pausedAt: now,
        readyAt,
        continueAt,
        isContinuingWhenReady: false,
        rates: {
          [window.key]: rate
        },
        restUntil,
        restMs: restUntil === null ? null : restUntil - now,
        hasSentPauseRequest: !isInterrupting
      }
    });
    record({
      id: session.id,
      text: restUntil === null ? "stopped" : `resting until ${spokenTime({
        time: restUntil,
        now
      })}`
    });

    if (isInterrupting) {
      await sendMessage(Command.ptyWrite, {
        id: session.id,
        data: ESCAPE
      });
      return;
    }

    await askToStop({
      id: session.id,
      window,
      isSlowing: isResting
    });
  }

  // The pause request: one extra turn, spent only where Esc can't stop the work -
  // background agents, workflows and teammates outlive the turn Esc ends.
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
      request: resumeRequest()
    });
  }

  // Pick a paused session back up - unless it is already working: a "continue"
  // typed into a running turn would queue a stray one behind it.
  async function pickUp({ id, request }: {
    id: string;
    request: string;
  }) {
    if (!paced.has(id)) {
      return;
    }

    forgetPace(id);
    workingSince.set(id, Date.now());
    record({
      id,
      text: "continued"
    });

    if (isWorking(id)) {
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
      request: USER_CONTINUE_REQUEST
    });
  }

  // Back on pace. A session still finishing its step is left to stop first; one
  // the user took over is released; a resting one continues; a stopped one
  // continues at once or waits for the user, as `stopThen` says.
  async function release({ session, accountKey, state, pacing }: {
    session: AgentSession;
    accountKey: string;
    state: PaceState;
    pacing: PacingByWindow;
  }) {
    const id = session.id;
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

    const shouldContinue = isContinuingOnRoom({
      state,
      pacing
    });
    if (shouldContinue) {
      releasable.set(accountKey, [...releasable.get(accountKey) ?? [], session]);
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
  async function stepModel({ session, accountKey, binding, step }: {
    session: AgentSession;
    accountKey: string;
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

    steppedAccounts.add(accountKey);
    record({
      id: session.id,
      text: `${STEP_HISTORY_TEXT[step]} - now ${adaptedChoice(session.id)?.current ?? ""}`
    });
    return true;
  }

  // Take the first allowed step that still has somewhere to go.
  async function climb({ session, accountKey, binding }: {
    session: AgentSession;
    accountKey: string;
    binding: PaceBinding;
  }) {
    const allowed = allowedSteps(settingsOf(session)[binding.kind]);
    for (const step of allowed) {
      if (MODEL_STEPS.has(step)) {
        const hasStepped = await stepModel({
          session,
          accountKey,
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
        allowed,
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

  // A working session not yet paused, judged by its own project's settings:
  // over pace (its binding), or on pace with room to step a stepped-down choice
  // back up. What to do about it is the account's call (`shareAccount`).
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

    const marginOf = pacingMargins(settingsOf(session));
    const binding = assessPace({
      account,
      now,
      isPaused: false,
      marginOf,
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

    return {
      session,
      accountKey,
      binding,
      canStepUp: binding === null && adapter.isSteppedDown(session.id) && hasRoomToSpare({
        account,
        now,
        marginOf,
        rateOf
      })
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

  // What the session's next allowed step would save, as a fraction of its own
  // load - the same first step `climb` takes: an effort or model step priced by
  // its cost weight, a rest or stop saving all of it while it lasts.
  function nextStepSaving({ session, binding }: {
    session: AgentSession;
    binding: PaceBinding;
  }) {
    const running = sessionModel(session.id);
    const options = modelOptions(session.agent.command);
    for (const step of allowedSteps(settingsOf(session)[binding.kind])) {
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

  // One account's working sessions, judged together: its burn is everything on
  // it - every session and every subagent - so over pace, the steps go where
  // they save the most until they cover what the account must shed
  // (`planShedding`); on pace, one stepped-down session climbs back, the
  // lightest first. A step starts the account's burn rate over once the scan ends.
  async function shareAccount(assessments: readonly WorkingAssessment[]) {
    // eslint-disable-next-line svelte/prefer-svelte-reactivity -- one scan's reads, never rendered
    const loads = new Map(assessments.map(assessment => [assessment.session.id, loadOf(assessment.session)] as const));
    const overPace = assessments.flatMap(({ binding, ...rest }) => binding === null ? [] : [{
      ...rest,
      binding
    }]);
    if (overPace.length === 0) {
      await restoreLightest({
        assessments,
        loads
      });
      return;
    }

    let totalLoad = 0;
    for (const load of loads.values()) {
      totalLoad += load;
    }

    const chosen = planShedding({
      candidates: overPace.map(({ session, binding }) => ({
        id: session.id,
        load: loads.get(session.id) ?? 0,
        saving: nextStepSaving({
          session,
          binding
        })
      })),
      totalLoad,
      fraction: largestShed(overPace.map(({ binding }) => binding))
    });
    for (const { session, accountKey, binding } of overPace) {
      if (chosen.includes(session.id)) {
        await climb({
          session,
          accountKey,
          binding
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

  async function restoreLightest({ assessments, loads }: {
    assessments: readonly WorkingAssessment[];
    loads: ReadonlyMap<string, number>;
  }) {
    const restorable = assessments.filter(assessment => assessment.canStepUp);
    const id = lightestToRestore(
      restorable.map(assessment => ({
        id: assessment.session.id,
        load: loads.get(assessment.session.id) ?? 0
      }))
    );
    const chosen = restorable.find(assessment => assessment.session.id === id);
    if (chosen === undefined) {
      return;
    }

    steppedAccounts.add(chosen.accountKey);
    await adapter.stepUp(chosen.session);
    record({
      id: chosen.session.id,
      text: "stepped back up"
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

  // A paused session: continue it at the end of its rest or its window, or move
  // it along as its runway reaches the reset. An unreadable account leaves it be.
  async function advancePaced({ session, state, account, accountKey }: {
    session: AgentSession;
    state: PaceState;
    account: AccountUsage | null;
    accountKey: string;
  }) {
    const id = session.id;
    const pacing = settingsOf(session);
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
      marginOf: pacingMargins(pacing),
      rateOf: window => state.rates[window.key] ?? liveRate({
        accountKey,
        window
      })
    });
    if (binding === null) {
      await release({
        session,
        accountKey,
        state,
        pacing
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
  // scan): paced by its own project's settings, or stood down when they're off.
  // A working one not yet paused comes back as its assessment for its account.
  async function lookAt({ session, accounts, sampled, now }: {
    session: AgentSession;
    accounts: Map<string, Promise<AccountUsage | null>>;
    sampled: Set<string>;
    now: number;
  }) {
    if (!isPacingOn(settingsOf(session))) {
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
    const measurable = sessions.filter(session => isWorking(session.id) && isPacingOn(settingsOf(session)));
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
    const scales: {
      measured: number;
      estimated: number;
    }[] = [];
    for (const { session, measured } of readings) {
      if (measured === null) {
        continue;
      }

      measuredLoads.set(session.id, measured);
      scales.push({
        measured,
        estimated: estimatedLoad(session)
      });
    }

    estimateScale = measuredPerEstimated(scales);
  }

  // One look at every session worth a look - working ones that may need pacing,
  // paced ones that may continue - then each account's working sessions shared
  // out together. The accounts stepped this scan start their burn rate over.
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
    // eslint-disable-next-line svelte/prefer-svelte-reactivity -- one scan's bookkeeping, never rendered
    const byAccount = new Map<string, WorkingAssessment[]>();
    for (const session of sessions) {
      const isWorthALook = paced.has(session.id) || isWorking(session.id) || adapter.isSteppedDown(session.id);
      const assessment = isWorthALook ? await lookAt({
        session,
        accounts,
        sampled,
        now
      }) : null;
      if (assessment !== null) {
        byAccount.set(assessment.accountKey, [...byAccount.get(assessment.accountKey) ?? [], assessment]);
      }
    }

    for (const assessments of byAccount.values()) {
      await shareAccount(assessments);
    }

    await releaseOnePerAccount();

    for (const accountKey of steppedAccounts) {
      forgetSamples(accountKey);
    }

    steppedAccounts.clear();
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

        const isReady = state.phase === PacePhase.enum.ready;
        const isContinuingWhenReady = isContinuingOnRoom({
          state,
          pacing: settingsOf(session)
        });
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
    /** The user picks a paused session back up now, and pacing lets that run be. */
    continueNow: continueAnyway,
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
      controller = null;
      pacingSource = null;
      clearTimeout(timer);
      timer = undefined;
    }
  };
}
