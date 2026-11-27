// A `/goal` across a handoff. A goal keeps the agent from ending its turn until
// its condition holds, so a session handing off with one running would never
// settle to write its doc. The handoff stops the goal's turn and clears it,
// then sets the same goal in the successor once that one has read the doc and
// settled - setting a goal starts a turn at once, which must not run ahead of
// the successor learning what it continues.

import { submitTypedCommand } from "@/lib/agent/submit-prompt";
import { Command, sendMessage } from "@/lib/ipc/messaging";
import { whenAgentSettled } from "@/lib/stores/agent/agentActivity.svelte";
import type { HandoffContext } from "@/lib/stores/agent/handoff/handoff-doc-request";
import { isGoalActive } from "@/lib/stores/agent/sessionGoals.svelte";
import { ESCAPE } from "@/lib/terminal/input/terminal-sequences";
import type { AgentSession } from "@/lib/types/agent";

const GOAL_COMMAND = "/goal";
const CLEAR_GOAL_COMMAND = `${GOAL_COMMAND} clear`;
/** The longest a stopped or newly started agent is waited on to settle - a
 *  turn that never settles must not hold the handoff for good. */
const SETTLE_PATIENCE_MS = 60_000;

/** Resolve once the agent settles, or after `SETTLE_PATIENCE_MS` either way. */
async function settledOrPatient({ context, id }: { context: HandoffContext; id: string }) {
  await Promise.race([whenAgentSettled(id), context.timers.afterDelay(SETTLE_PATIENCE_MS)]);
}

/** Stop and clear the session's running goal, returning its condition to set
 *  again later - or null when it runs none, or its condition can't be read
 *  back (then it is left running: a goal cleared for good is the user's work
 *  lost). */
export async function suspendGoal({ context, session }: { context: HandoffContext; session: AgentSession }) {
  if (!isGoalActive(session.id)) {
    return null;
  }

  const condition = await sendMessage(Command.AGENT_SESSION_GOAL, {
    command: session.agent.command,
    conversationId: session.conversationId
  });
  if (condition === null) {
    return null;
  }

  await sendMessage(Command.PTY_WRITE, {
    id: session.id,
    data: ESCAPE
  });
  await settledOrPatient({
    context,
    id: session.id
  });
  await submitTypedCommand({
    id: session.id,
    command: CLEAR_GOAL_COMMAND,
    wait: context.timers.afterDelay
  });
  return condition;
}

/** Set `condition` as the goal of session `id` once it has settled. */
export async function restoreGoal({
  context,
  id,
  condition
}: {
  context: HandoffContext;
  id: string;
  condition: string;
}) {
  await settledOrPatient({
    context,
    id
  });
  await submitTypedCommand({
    id,
    command: `${GOAL_COMMAND} ${condition}`,
    wait: context.timers.afterDelay
  });
}
