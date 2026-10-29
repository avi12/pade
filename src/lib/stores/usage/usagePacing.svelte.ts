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
import { windowLabel } from "@/lib/ipc/platform";
import { registerSessionState, SessionStateKey } from "@/lib/session/lifecycle/session-scope";
import { agentReportsBusy } from "@/lib/stores/agent/agentActivity.svelte";
import { isModelSwitchApplying } from "@/lib/stores/model/modelSwitch.svelte";
import { sessionStatus } from "@/lib/stores/session/sessions.svelte";
import { adaptedChoice, adaptedSteps, createUsageAdapter } from "@/lib/stores/usage/usageAdapt.svelte";
import type { AdaptHost } from "@/lib/stores/usage/usageAdapt.svelte";
import { hasUsageLimit, readAccount } from "@/lib/stores/usage/usageResume.svelte";
import { PacedWindowKind, PacingStep, PacingStopThen, SessionStatus } from "@/lib/types";
import type { AccountUsage, AgentSession, PacingByWindow, UsageWindow } from "@/lib/types";
import {
  allowedSteps,
  isPacingOn,
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
const MILLISECONDS_PER_MINUTE = 60_000;
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
  restMs: z.number().nullable().default(null)
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
/** Each agent's account forecast per window, for the Config panel's live lines. */
const forecasts = new SvelteMap<string, Partial<Record<PacedWindowKind, PaceForecast>>>();
/** The live loop's settings, so a badge counts the steps its window allows. */
let pacingSource: (() => PacingByWindow) | null = null;
/** When each working session's current stretch of work began - how long a
 *  resting session has just worked, which sizes its rest. */
// eslint-disable-next-line svelte/prefer-svelte-reactivity -- written per scan, never rendered
const workingSince = new Map<string, number>();
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
    workingSince.delete(id);
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
    time: isContinuingWhenReady ? state.readyAt : state.continueAt,
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
  if (pacingSource === null) {
    return 0;
  }

  const kind = paced.get(id)?.windowKind ?? verdicts.get(id)?.windowKind ?? PacedWindowKind.enum.session;
  return allowedSteps(pacingSource()[kind]).length;
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
  /** This window's workspace's settings, per paced window. */
  settings: () => PacingByWindow;
  /** Whether the auto-handoff flow owns the session right now. */
  isHandingOff: (id: string) => boolean;
}

/** The pacing loop, scoped to one app shell. The shell calls `check()` from a
 *  `$effect` and `dispose()` on destroy, and shows `pauses` while sessions wait. */
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

  for (const [key, readings] of Object.entries(
    readStoredJson({
      key: samplesStorageKey(),
      schema: StoredSamples
    }) ?? {}
  )) {
    samples.set(key, readings);
  }

  pacingSource = host.settings;

  function margins() {
    return pacingMargins(host.settings());
  }

  function sessionOf(id: string) {
    return host.sessions().find(session => session.id === id);
  }

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
        restMs: restUntil === null ? null : restUntil - now
      }
    });
    record({
      id: session.id,
      text: restUntil === null ? "stopped" : `resting until ${spokenTime({
        time: restUntil,
        now
      })}`
    });
    await submitPastedPrompt({
      id: session.id,
      text: pauseRequest({
        window,
        now,
        isSlowing: isResting
      }),
      wait: afterDelay
    });
  }

  // Pick a paused session back up - unless it is already working: a "continue"
  // typed into a running turn would queue a stray one behind it.
  async function resume(id: string) {
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
      text: resumeRequest(),
      wait: afterDelay
    });
  }

  // Back on pace. A session still finishing its step is left to stop first; one
  // the user took over is released; a resting one continues; a stopped one
  // continues at once or waits for the user, as `stopThen` says.
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

    const shouldContinue = host.settings()[state.windowKind].stopThen === PacingStopThen.enum.continue
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

    forgetSamples(accountKey);
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
    const allowed = allowedSteps(host.settings()[binding.kind]);
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

  // A working session not yet paused. Over pace: the next allowed step. On pace
  // with room to spare: a stepped-down session climbs a notch back. Either step
  // starts its account's burn rate over, so the next one waits on a fresh measure.
  async function paceWorking({ session, account, accountKey }: {
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

    const marginOf = margins();
    const binding = assessPace({
      account,
      now,
      isPaused: false,
      marginOf,
      rateOf
    });
    if (binding === null) {
      verdicts.delete(session.id);
      const canStepUp = adapter.isSteppedDown(session.id) && hasRoomToSpare({
        account,
        now,
        marginOf,
        rateOf
      });
      if (canStepUp) {
        forgetSamples(accountKey);
        await adapter.stepUp(session);
        record({
          id: session.id,
          text: "stepped back up"
        });
      }

      return;
    }

    verdicts.set(session.id, {
      windowKind: binding.kind,
      utilization: binding.window.utilization,
      runOutAt: now + (binding.continueAt - binding.readyAt),
      readyAt: binding.readyAt,
      resetAt: binding.continueAt
    });
    await climb({
      session,
      accountKey,
      binding
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
  async function advancePaced({ id, state, account, accountKey }: {
    id: string;
    state: PaceState;
    account: AccountUsage | null;
    accountKey: string;
  }) {
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
      marginOf: margins(),
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

    const isSkipped = (skippedUntil.get(id) ?? 0) > Date.now();
    if (isSkipped || isModelSwitchApplying(id)) {
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
      isEveryone: false
    });

    // eslint-disable-next-line svelte/prefer-svelte-reactivity -- one scan's reads, never rendered
    const accounts = new Map<string, Promise<AccountUsage | null>>();
    // eslint-disable-next-line svelte/prefer-svelte-reactivity -- one scan's bookkeeping, never rendered
    const sampled = new Set<string>();
    for (const session of sessions) {
      const isWorthALook = paced.has(session.id) || isWorking(session.id) || adapter.isSteppedDown(session.id);
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
      accountKeys.set(session.id, accountKey);
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
    for (const id of paced.keys()) {
      await resume(id);
    }

    verdicts.clear();
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

    if (isPacingOn(host.settings())) {
      await scan();
    } else {
      await standDown();
    }

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
        await resume(id);
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
    resume
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
        const isContinuingWhenReady = host.settings()[state.windowKind].stopThen === PacingStopThen.enum.continue
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
      controller = null;
      pacingSource = null;
      clearTimeout(timer);
      timer = undefined;
    }
  };
}
