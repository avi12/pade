// A working session's measured load: what it and its subagents spent over the
// last few minutes, priced (`token-price`) and per minute - read by the backend
// from the agent's own transcripts. An agent that keeps none, a session with no
// reply in the window, or a read that fails measures nothing (null), and usage
// pacing estimates that session's load instead.
//
// The subagents still running come apart from the rest, each with its own load,
// so pacing can hold one back on its own; a subagent that has finished stays in
// the session's own share - what it spent was the session's turn.

import { Command, sendMessage } from "@/lib/ipc/messaging";
import type { AgentSession, SessionTokenUsage } from "@/lib/types/agent";
import { MILLISECONDS_PER_MINUTE } from "@/lib/usage/pacing/engine/pacing-policy";
import { weightedTokens } from "@/lib/usage/pacing/engine/token-price";

/** How far back a session's burn is measured: a few turns and the subagents
 *  they ran, yet short enough to follow a model or effort switch soon after. */
const MEASURED_BURN_WINDOW_MINUTES = 20;
/** A subagent whose transcript was written this recently is still running -
 *  a long tool call can keep one quiet this long, no longer. */
export const RUNNING_SUBAGENT_MS = 3 * MILLISECONDS_PER_MINUTE;

/** A session's measured burn, split into its own share and each running
 *  subagent's, in priced tokens per minute over the window - null when nothing
 *  was spent. Pure. */
export function splitBurn({ usage, now }: { usage: SessionTokenUsage; now: number }) {
  let own = weightedTokens(usage.own) / MEASURED_BURN_WINDOW_MINUTES;
  const running = [];
  for (const subagent of usage.subagents) {
    const load = weightedTokens(subagent.usage) / MEASURED_BURN_WINDOW_MINUTES;
    const isRunning = now - subagent.writtenAt <= RUNNING_SUBAGENT_MS;
    if (!isRunning) {
      own += load;
      continue;
    }

    running.push({
      id: subagent.id,
      description: subagent.description,
      load
    });
  }

  let total = own;
  for (const subagent of running) {
    total += subagent.load;
  }

  return total > 0
    ? {
        total,
        own,
        subagents: running
      }
    : null;
}

export type MeasuredBurn = NonNullable<ReturnType<typeof splitBurn>>;

/** The session's subagents that wrote to their transcripts since `since`
 *  (epoch ms), each with when it last did - empty for an agent that keeps no
 *  transcripts. Never rejects: a failed read finds none. */
export async function subagentsWrittenSince({ session, since }: { session: AgentSession; since: number }) {
  try {
    const usage = await sendMessage(Command.sessionTokenUsage, {
      command: session.agent.command,
      conversationId: session.conversationId,
      since
    });
    return usage.subagents
      .filter(subagent => subagent.writtenAt >= since)
      .map(({ id, description, writtenAt }) => ({
        id,
        description,
        writtenAt
      }));
  } catch (error) {
    console.warn("pade: could not read the session's subagent activity", error);
    return [];
  }
}

/** The session's measured burn, or null when nothing was measured. Never
 *  rejects: a failed read is a missing measurement. */
export async function measuredBurn({ session, now }: { session: AgentSession; now: number }) {
  try {
    const usage = await sendMessage(Command.sessionTokenUsage, {
      command: session.agent.command,
      conversationId: session.conversationId,
      since: now - MEASURED_BURN_WINDOW_MINUTES * MILLISECONDS_PER_MINUTE
    });
    return splitBurn({
      usage,
      now
    });
  } catch (error) {
    console.warn("pade: could not measure the session's token burn, estimating it instead", error);
    return null;
  }
}
