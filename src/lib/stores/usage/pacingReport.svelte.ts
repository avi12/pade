// The window's pacing scan, reported to the backend engine. In the shadow phase
// the engine decides what it would do from these facts and compares it with
// what this window's loop actually decided (`recordObserved`); it acts on
// nothing. Facts read from where they already live - the loop owns its own
// (`OwnedFacts`), the rest comes from the app's stores - and the shaping is
// lib/usage/pacing/shadow-report/report-facts.

import { Command, sendMessage } from "@/lib/ipc/messaging";
import { windowLabel } from "@/lib/ipc/platform";
import { backgroundAgentCount } from "@/lib/stores/agent/agentActivity.svelte";
import { sessionModel } from "@/lib/stores/agent/context.svelte";
import { modelOptions } from "@/lib/stores/model/modelOptions.svelte";
import { isModelSwitchApplying } from "@/lib/stores/model/modelSwitch.svelte";
import { sessionLabel } from "@/lib/stores/session/sessionLabels.svelte";
import { adaptedChoice, adaptedSteps, originalChoice } from "@/lib/stores/usage/usageAdapt.svelte";
import { hasUsageLimit } from "@/lib/stores/usage/usageResume.svelte";
import type { AgentSession } from "@/lib/types/agent";
import { createObservedRecorder } from "@/lib/usage/pacing/shadow-report/observed-decisions";
import type { ObservedKind, PacingReport } from "@/lib/usage/pacing/shadow-report/pacing-report";
import { choiceFactsOf, sessionFactsOf } from "@/lib/usage/pacing/shadow-report/report-facts";
import type { OwnedFacts } from "@/lib/usage/pacing/shadow-report/report-facts";

const recorder = createObservedRecorder();

/** The pacing loop took a decision on a session. */
export function recordObserved({ sessionId, kind }: { sessionId: string; kind: ObservedKind }) {
  recorder.note({
    sessionId,
    kind,
    at: Date.now()
  });
}

/** The pacing loop weighed a session this scan, whether or not it acted. */
export function markJudged(sessionId: string) {
  recorder.markJudged(sessionId);
}

function choiceOf(session: AgentSession) {
  const original = originalChoice(session.id);
  return choiceFactsOf({
    running: sessionModel(session.id),
    original:
      original === null
        ? null
        : {
            model: original.model ?? null,
            effort: original.effort ?? null
          },
    restoreAt: adaptedChoice(session.id)?.restoreAt ?? null,
    isSteppedDown: adaptedSteps(session.id).length > 0,
    options: modelOptions(session.agent.command)
  });
}

/** Send this scan's facts and the decisions taken since the last one. A
 *  report that can't be sent is dropped: reporting never stops the loop. */
export async function sendPacingReport({
  sessions,
  at,
  isPacingOn,
  isHandingOff,
  ownedOf
}: {
  sessions: readonly AgentSession[];
  at: number;
  isPacingOn: boolean;
  isHandingOff: (id: string) => boolean;
  ownedOf: (session: AgentSession) => OwnedFacts;
}) {
  try {
    const report: PacingReport = {
      windowLabel: windowLabel(),
      at,
      sessions: sessions.map(session =>
        sessionFactsOf({
          reported: {
            id: session.id,
            conversationId: session.conversationId ?? null,
            agentId: session.agent.id,
            command: session.agent.command,
            name: sessionLabel(session.id) ?? session.agent.label,
            backgroundAgentCount: backgroundAgentCount(session.id),
            isPacingOn,
            isHandingOff: isHandingOff(session.id),
            hasUsageLimit: hasUsageLimit(session.id),
            isModelSwitchApplying: isModelSwitchApplying(session.id),
            choice: choiceOf(session)
          },
          owned: ownedOf(session)
        })
      ),
      observed: recorder.drain(at)
    };
    await sendMessage(Command.pacingReport, report);
  } catch (error) {
    console.warn("pade: could not send the pacing report, dropping it", error);
  }
}
