import { Command, sendMessage } from "@/lib/ipc/messaging";
import { HandoffReason } from "@/lib/stores/agent/handoff/handoff-prompts";
import { fetchAccountReading } from "@/lib/stores/usage/accountReadings.svelte";
import type { Agent } from "@/lib/types/agent";
import { CreditsState } from "@/lib/types/usage";
import { USAGE_SPENT_PERCENTAGE } from "@/lib/usage/groups/limit-ranking";
import { settledValue } from "@/lib/util/async/settled";

/** Whether an agent can still do work: its headline quota isn't spent, or usage
 *  credits carry it past that cap (a spent weekly window is no obstacle while
 *  extra usage is on and has room). An unknown quota - a tier-label account with
 *  no numbers - counts as "enough" so the feature still works. This is what keeps
 *  a handoff with the same agent: only an agent that truly cannot work hands its
 *  session to a different one. */
export function hasUsageHeadroom({
  usedPercentage,
  credits
}: {
  usedPercentage: number | null | undefined;
  credits: CreditsState | null | undefined;
}) {
  if (usedPercentage == null) {
    return true;
  }

  const quotaLeft = usedPercentage < USAGE_SPENT_PERCENTAGE;
  return quotaLeft || credits === CreditsState.enum.available;
}

/** Pick the agent that should take over. The current agent stays on while it
 *  still has usage headroom (the context-driven handoff - context near full but
 *  the agent can keep going). Once it's out of headroom, the first OTHER
 *  available agent that has headroom takes over (a usage crossover). `null` when
 *  no agent has headroom - the caller stays marked and skips this cycle. */
export async function pickSuccessor({
  current,
  available,
  hasHeadroom
}: {
  current: Agent;
  available: Agent[];
  hasHeadroom: (agentId: string) => Promise<boolean>;
}) {
  if (await hasHeadroom(current.id)) {
    return current;
  }

  for (const agent of available) {
    if (agent.id === current.id) {
      continue;
    }

    if (await hasHeadroom(agent.id)) {
      return agent;
    }
  }

  return null;
}

/** Pick the successor for a specific handoff trigger. Configuration changes
 * must relaunch the governed agent itself so its fresh process reads that
 * agent's changed config; context handoffs retain quota-based crossover. */
export async function pickHandoffSuccessor({
  reason,
  current,
  available,
  hasHeadroom
}: {
  reason: HandoffReason;
  current: Agent;
  available: Agent[];
  hasHeadroom: (agentId: string) => Promise<boolean>;
}) {
  if (reason === HandoffReason.ConfigurationChange) {
    return current;
  }

  return await pickSuccessor({
    current,
    available,
    hasHeadroom
  });
}

// Only cycle when there's quota to spare - a handoff itself costs tokens.
export async function hasEnoughUsage(agent: string) {
  const quota = await settledValue({
    promise: sendMessage(Command.usageGet, { agent }),
    fallback: null
  });
  const reading = await settledValue({
    promise: fetchAccountReading({ agent }),
    fallback: null
  });
  return hasUsageHeadroom({
    usedPercentage: quota?.usedPct,
    credits: reading?.account?.credits
  });
}
