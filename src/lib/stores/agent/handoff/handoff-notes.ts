import { measuredContextPercentage } from "@/lib/stores/agent/context.svelte";
import { HandoffReason } from "@/lib/stores/agent/handoff/handoff-prompts";
import type { Agent, AgentSession } from "@/lib/types/agent";

/** The status line while a handoff waits for the agent's running work - a turn,
 *  background agents, a dynamic workflow - to finish. */
export function waitingForWorkNote({ session, reason }: { session: AgentSession; reason: HandoffReason }) {
  const agent = session.agent.label;
  if (reason === HandoffReason.ConfigurationChange) {
    return `MCP servers changed - ${agent} restarts once its running work finishes…`;
  }

  const measuredPercentage = Math.round(measuredContextPercentage(session.id) ?? 0);
  return `${agent} context at ${measuredPercentage}% - handing off once its running work finishes, or just before it auto-compacts…`;
}

/** The status line while a handoff is under way. The context note states the
 *  measured number: an unquantified "nearly full" reads as a lie whenever the
 *  user is looking at a different (or fresher) session than the one that hit
 *  the threshold. */
export function handingOffNote({
  session,
  reason,
  isMidTurn,
  successorAgent
}: {
  session: AgentSession;
  reason: HandoffReason;
  isMidTurn: boolean;
  successorAgent: Agent;
}) {
  const agent = session.agent.label;
  if (reason === HandoffReason.ConfigurationChange) {
    return `MCP servers changed - ${agent} is writing a handoff before restarting…`;
  }

  if (isMidTurn) {
    return `${agent} is about to auto-compact - handing off before it does…`;
  }

  const isCrossover = successorAgent.id !== session.agent.id;
  if (isCrossover) {
    return `${agent} is out of usage - handing off to ${successorAgent.label}…`;
  }

  const measuredPercentage = Math.round(measuredContextPercentage(session.id) ?? 0);
  return `${agent} context at ${measuredPercentage}% - handing off to a fresh agent…`;
}
