// A working session's measured load: what it and its subagents spent over the
// last few minutes, priced (`token-price`) and per minute - read by the backend
// from the agent's own transcripts. An agent that keeps none, a session with no
// reply in the window, or a read that fails measures nothing (null), and usage
// pacing estimates that session's load instead.

import { Command, sendMessage } from "@/lib/ipc/messaging";
import type { AgentSession } from "@/lib/types";
import { MILLISECONDS_PER_MINUTE } from "@/lib/usage/pacing/pacing-policy";
import { weightedTokens } from "@/lib/usage/pacing/token-price";

/** How far back a session's burn is measured: a few turns and the subagents
 *  they ran, yet short enough to follow a model or effort switch soon after. */
const MEASURED_BURN_WINDOW_MINUTES = 20;

/** The session's priced tokens per minute over the window, or null when
 *  nothing was measured. Never rejects: a failed read is a missing measurement. */
export async function measuredBurn({ session, now }: {
  session: AgentSession;
  now: number;
}) {
  try {
    const usage = await sendMessage(Command.sessionTokenUsage, {
      command: session.agent.command,
      conversationId: session.conversationId,
      since: now - MEASURED_BURN_WINDOW_MINUTES * MILLISECONDS_PER_MINUTE
    });
    if (usage.length === 0) {
      return null;
    }

    return weightedTokens(usage) / MEASURED_BURN_WINDOW_MINUTES;
  } catch (error) {
    console.warn("pade: could not measure the session's token burn, estimating it instead", error);
    return null;
  }
}
