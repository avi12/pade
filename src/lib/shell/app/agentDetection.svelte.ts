import { onMount } from "svelte";

import { Command, sendMessage } from "@/lib/ipc/messaging";
import { realAgents as toRealAgents } from "@/lib/types/agent";
import type { Agent } from "@/lib/types/agent";
import { settledValue } from "@/lib/util/async/settled";

// Agent detection runs a subprocess per agent, so cap how long the boot waits
// on it - a stall must never freeze the splash. An empty list just routes to
// onboarding, and the redetect interval fills it in shortly after.
const BOOT_DETECT_TIMEOUT_MS = 6_000;
// The slow fallback poll behind the visibility-driven re-detect.
const AGENT_REDETECT_INTERVAL_MS = 30_000;

/** The installed agents, detected at boot and kept fresh afterwards. */
export function createAgentDetection() {
  let agents = $state<Agent[]>([]);
  // Agents excluding the always-present shell fallback - this count decides
  // whether we auto-launch or onboard.
  const realAgents = $derived(toRealAgents(agents));
  // Detection order is the registry's priority order, so the first real agent
  // is the best installed one; the shell fallback carries otherwise.
  const bestAgent = $derived(realAgents[0] ?? agents[0]);

  async function detectWithinBootBudget() {
    // Detection is best-effort: a rejection or a stall both yield an empty
    // list rather than blocking the routing that follows.
    agents = await Promise.race([
      settledValue({
        promise: sendMessage(Command.agentsDetect),
        fallback: []
      }),
      new Promise<Agent[]>(resolve => setTimeout(() => resolve([]), BOOT_DETECT_TIMEOUT_MS))
    ]);
  }

  // Re-detect installed agents so the picker reflects an agent the user just
  // installed or removed - when the app becomes visible again (they switched back
  // from installing one) and on a slow poll as a fallback. Detection spawns a
  // process per agent, so it must stay off the drag path: page visibility never
  // changes while dragging a window that stays on screen, unlike window focus
  // which a title-bar drag churns and lagged. Best-effort and never rejecting -
  // the poll and the visibility signal both fire it without awaiting - so a
  // transient failure keeps the agents already on screen.
  async function redetectAgents() {
    agents = await settledValue({
      promise: sendMessage(Command.agentsDetect),
      fallback: agents
    });
  }

  onMount(() => {
    const interval = setInterval(() => redetectAgents(), AGENT_REDETECT_INTERVAL_MS);
    return () => clearInterval(interval);
  });

  return {
    get agents() {
      return agents;
    },
    get realAgents() {
      return realAgents;
    },
    get bestAgent() {
      return bestAgent;
    },
    detectWithinBootBudget,
    redetectAgents
  };
}

export type AgentDetection = ReturnType<typeof createAgentDetection>;
