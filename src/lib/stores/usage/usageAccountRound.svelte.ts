// The account-wide pacing round between PADE's windows (the rules are
// lib/usage/pacing/account-round). Every window's pacing loop submits a report
// of its working sessions each scan; it reaches the other windows on one
// BroadcastChannel. The leader - the first label among the windows reporting -
// plans the round over all of them, writes every session's throttle (its
// subagents' steps and the spawn block) to that session's pacing control, and
// broadcasts the plan; each window then takes the session steps that are its
// own. The round's subagent steps are kept in localStorage, which the windows
// share, so a new leader picks up where a closed one stopped.
//
// One elected window rather than the backend: the round needs each window's
// own judgement of its sessions (their project's settings, their model steps,
// their stops), all of which live in that window's stores - the backend would
// have to learn all of it to decide, while a window only needs everyone's
// reports.

import { windowLabel } from "@/lib/ipc/platform";
import { NO_THROTTLE, setThrottlePart, showThrottles } from "@/lib/stores/usage/pacingControl.svelte";
import type { ControlTarget } from "@/lib/stores/usage/pacingControl.svelte";
import {
  controlThrottle,
  freshReports,
  PACING_CHANNEL,
  PacingMessageKind,
  parsePacingMessage,
  planRound,
  reportLifetime,
  roundLeader,
  SubagentSteps
} from "@/lib/usage/pacing/account-round";
import type { PacingMessage, RoundPlan, WindowReport } from "@/lib/usage/pacing/account-round";
import { readStoredJson, writeStoredJson } from "@/lib/util/data/stored-json";

const SUBAGENT_STEPS_STORAGE_KEY = "pade.usage-pacing-subagents";

export function createAccountRound({ pollMs, onPlan }: {
  /** How often each window submits - its reports live a couple of these. */
  pollMs: number;
  /** Take the plan's steps that are this window's own. */
  onPlan: (plan: RoundPlan) => Promise<void>;
}) {
  const channel = new BroadcastChannel(PACING_CHANNEL);
  const ownLabel = windowLabel();
  const lifetimeMs = reportLifetime(pollMs);
  // eslint-disable-next-line svelte/prefer-svelte-reactivity -- the round's inputs, never rendered
  const reports = new Map<string, WindowReport>();
  /** When this window's rounds last stepped each session. */
  // eslint-disable-next-line svelte/prefer-svelte-reactivity -- the round's memory, never rendered
  const steppedAt = new Map<string, number>();
  /** The sessions this window last throttled as leader, to lift the throttle
   *  of one that leaves the round. */
  // eslint-disable-next-line svelte/prefer-svelte-reactivity -- the round's memory, never rendered
  let throttled = new Map<string, ControlTarget>();

  function post(message: PacingMessage) {
    channel.postMessage(message);
  }

  channel.onmessage = async e => {
    const parsed = parsePacingMessage(e.data);
    if (!parsed.success) {
      return;
    }

    const message = parsed.data;
    if (message.kind === PacingMessageKind.enum.report) {
      reports.set(message.report.label, message.report);
      return;
    }

    if (message.kind === PacingMessageKind.enum.gone) {
      reports.delete(message.label);
      return;
    }

    if (message.plan.leader === ownLabel) {
      return;
    }

    // Another window leads now: what this one throttled is theirs to lift.
    // eslint-disable-next-line svelte/prefer-svelte-reactivity -- the round's memory, never rendered
    throttled = new Map();
    showThrottles(message.plan.throttles);
    await onPlan(message.plan);
  };

  /** Write every reported session's throttle, and lift it from one that left
   *  the round since. */
  async function writeThrottles({ fresh, plan }: {
    fresh: readonly WindowReport[];
    plan: RoundPlan;
  }) {
    // eslint-disable-next-line svelte/prefer-svelte-reactivity -- one round's targets, never rendered
    const targets = new Map<string, ControlTarget>();
    for (const report of fresh) {
      for (const session of report.sessions) {
        targets.set(session.id, {
          command: session.command,
          conversationId: session.conversationId ?? undefined
        });
      }
    }

    for (const throttle of plan.throttles) {
      const target = targets.get(throttle.sessionId);
      if (target !== undefined) {
        await setThrottlePart({
          target,
          throttle: controlThrottle(throttle)
        });
      }
    }

    for (const [id, target] of throttled) {
      if (!targets.has(id)) {
        await setThrottlePart({
          target,
          throttle: NO_THROTTLE
        });
      }
    }

    throttled = targets;
  }

  async function lead({ fresh, now }: {
    fresh: readonly WindowReport[];
    now: number;
  }) {
    const { plan, steps } = planRound({
      reports: fresh,
      steps: readStoredJson({
        key: SUBAGENT_STEPS_STORAGE_KEY,
        schema: SubagentSteps
      }) ?? {},
      steppedAt,
      leader: ownLabel,
      now
    });
    writeStoredJson({
      key: SUBAGENT_STEPS_STORAGE_KEY,
      value: steps
    });
    for (const id of plan.climb) {
      steppedAt.set(id, now);
    }

    await writeThrottles({
      fresh,
      plan
    });
    post({
      kind: PacingMessageKind.enum.plan,
      plan
    });
    showThrottles(plan.throttles);
    await onPlan(plan);
  }

  return {
    /** This window's sessions for the round; the leader plans it at once. */
    async submit(report: WindowReport) {
      reports.set(ownLabel, report);
      post({
        kind: PacingMessageKind.enum.report,
        report
      });
      const fresh = freshReports({
        reports: reports.values(),
        now: report.at,
        lifetimeMs
      });
      if (roundLeader(fresh) === ownLabel) {
        await lead({
          fresh,
          now: report.at
        });
      }
    },
    dispose() {
      post({
        kind: PacingMessageKind.enum.gone,
        label: ownLabel
      });
      channel.close();
    }
  };
}
