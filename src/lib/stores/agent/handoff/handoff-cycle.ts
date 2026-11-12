// One handoff, start to finish: wait out the agent's running work, pick the
// successor, get the doc written, end the session, launch the successor seeded
// from the doc and, once it has used it, delete the doc.

import { Command, sendMessage } from "@/lib/ipc/messaging";
import {
  askForDoc,
  docPathOf,
  isStillOpen,
  requestHandoffDoc,
  waitForRunningWorkUnlessMidTurn
} from "@/lib/stores/agent/handoff/handoff-doc-request";
import type { HandoffContext } from "@/lib/stores/agent/handoff/handoff-doc-request";
import { waitForFirstTurn } from "@/lib/stores/agent/handoff/handoff-first-turn";
import { handingOffNote } from "@/lib/stores/agent/handoff/handoff-notes";
import { handoffDocName, HandoffReason, successorPrompt } from "@/lib/stores/agent/handoff/handoff-prompts";
import { hasEnoughUsage, pickHandoffSuccessor } from "@/lib/stores/agent/handoff/handoff-successor";
import { shieldHandoffSuccessor } from "@/lib/stores/usage/usagePacing.svelte";
import type { Agent, AgentSession } from "@/lib/types/agent";

function docNameFor({ context, sessionId }: {
  context: HandoffContext;
  sessionId: string;
}) {
  return handoffDocName({
    source: context.host.slugSource(),
    sessionId
  });
}

// Same agent while it still has headroom; otherwise cross over to the first
// other available agent that does. No agent with headroom → stay marked so we
// don't re-check each tick; skip this cycle. Resolves the successor, or
// undefined when this cycle should stop.
async function successorForHandoff({ context, session, reason, isMidTurn }: {
  context: HandoffContext;
  session: AgentSession;
  reason: HandoffReason;
  isMidTurn: boolean;
}) {
  const isStillHereBeforeRequest = await waitForRunningWorkUnlessMidTurn({
    context,
    session,
    reason,
    isMidTurn
  });
  if (!isStillHereBeforeRequest) {
    context.state.abandonHandoff(session.id);
    return undefined;
  }

  return pickHandoffSuccessor({
    reason,
    current: session.agent,
    available: context.host.availableAgents(),
    hasHeadroom: hasEnoughUsage
  });
}

// End the session, start the successor seeded to continue, then delete the doc
// once the successor has really used it. Anything short of that keeps the doc:
// losing a handoff nobody read costs the whole session's work, where a
// leftover file costs one `rm`.
async function replaceWithSuccessor({ context, session, doc, successorAgent }: {
  context: HandoffContext;
  session: AgentSession;
  doc: string;
  successorAgent: Agent;
}) {
  const { host, state, timers } = context;
  const { cwd } = session;
  await host.endSession(session.id);
  // Launched before the predecessor is removed, which forgets everything kept
  // for it - the successor takes over its project, model and ✦ naming first.
  const successorId = host.launchSuccessor({
    agent: successorAgent,
    cwd,
    initialPrompt: successorPrompt(
      docPathOf({
        context,
        session,
        name: doc
      })
    ),
    predecessorId: session.id
  });
  shieldHandoffSuccessor(successorId);
  host.removeSession(session.id);
  state.unmarkHandingOff(session.id);
  state.note = "";

  const isConsumed = await waitForFirstTurn({
    timers,
    isStillOpen: id => isStillOpen({
      context,
      sessionId: id
    }),
    id: successorId
  });
  if (isConsumed) {
    await sendMessage(Command.handoffDocDelete, {
      dir: host.projectDirectory(session),
      name: doc
    });
  }
}

export async function handoff({ context, session, reason, isMidTurn }: {
  context: HandoffContext;
  session: AgentSession;
  reason: HandoffReason;
  isMidTurn: boolean;
}) {
  const { state } = context;
  const successorAgent = await successorForHandoff({
    context,
    session,
    reason,
    isMidTurn
  });
  if (!successorAgent) {
    return;
  }

  const doc = docNameFor({
    context,
    sessionId: session.id
  });
  state.note = handingOffNote({
    session,
    reason,
    isMidTurn,
    successorAgent
  });

  // 1. Ask the agent to write the handoff doc, then wait for it to land.
  const hasDoc = await requestHandoffDoc({
    context,
    session,
    doc,
    reason
  });
  if (!hasDoc) {
    state.note = "";
    return;
  }

  // 2. End the session - once the agent has finished the handoff turn and
  // anything it started meanwhile - 3. start the successor seeded to continue.
  const isStillHereBeforeEnd = await waitForRunningWorkUnlessMidTurn({
    context,
    session,
    reason,
    isMidTurn
  });
  if (!isStillHereBeforeEnd) {
    state.abandonHandoff(session.id);
    return;
  }

  await replaceWithSuccessor({
    context,
    session,
    doc,
    successorAgent
  });
}

// Ask `session`'s agent to write its handoff doc and resolve once the doc is on
// disk - the reusable core of the handoff, WITHOUT the kill/launch. The
// workspace-save flow uses it: the doc is written into the temp workspace, rides
// along in the copy to the saved project, and the successor there is seeded to
// continue from it. Returns the doc's file name, or null when the agent never
// produced it (the caller falls back to a plain restart). A doc a prior attempt
// already wrote is reused, not re-requested.
export async function writeHandoffDoc({ context, session }: {
  context: HandoffContext;
  session: AgentSession;
}) {
  const doc = docNameFor({
    context,
    sessionId: session.id
  });
  const isWritten = await askForDoc({
    context,
    session,
    doc,
    reason: HandoffReason.Save
  });
  return isWritten ? doc : null;
}
