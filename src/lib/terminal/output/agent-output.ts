// Hand an agent's PTY output to every store that reads meaning out of it.

import { observeAgentActivity } from "@/lib/stores/agent/agentActivity.svelte";
import { observeApiError } from "@/lib/stores/agent/apiErrorRetry.svelte";
import { observeContext, observeContextScreen } from "@/lib/stores/agent/context.svelte";
import { observeMcpReload } from "@/lib/stores/agent/mcpReload.svelte";
import { observeUsageLimit } from "@/lib/stores/usage/usageResume.svelte";

/** One live chunk of the session's output. */
export function observeAgentOutput({ id, chunk, isOnNormalScreen }: {
  id: string;
  chunk: string;
  isOnNormalScreen: boolean;
}) {
  // What the agent itself says about its work - a turn or background agents
  // and workflows - read from its title (gates session-ending flows).
  observeAgentActivity({
    id,
    chunk
  });

  // Track how full this agent's context window is (drives auto-handoff).
  // Only on the normal screen: a fullscreen agent repaints its whole frame
  // on every spinner tick, so counting those bytes balloons the estimate
  // to a meaningless 100% within minutes (a freshly handed-off successor
  // read "≈100%"). On the alternate screen the rendered-screen sample
  // carries the parsed percent, and no estimate beats a saturated one.
  if (isOnNormalScreen) {
    observeContext({
      id,
      chunk
    });
  } else {
    // Parse-only on the alternate screen: the window banner ("(1M
    // context)") paints once at spawn and then scrolls out of the visible
    // frame, so the screen sample alone would never learn the window size
    // and the gauge would sit on "measuring…" forever. Split-word parses
    // simply miss here; the screen sample still carries those.
    observeContextScreen({
      id,
      text: chunk
    });
  }

  // Spot the CLI's "limit reached" stop message (drives auto-resume).
  observeUsageLimit({
    id,
    chunk
  });
  // Spot a transient API-error stop (drives API-error auto-retry).
  observeApiError({
    id,
    chunk
  });
  observeMcpReload({
    id,
    chunk
  });
}

/** The session's replayed history, on attaching to it. It holds the spawn-time
 *  banner (window size) and the latest counters a fresh attach would otherwise
 *  never see - parse-only, never the byte estimate: these are replayed bytes,
 *  not new output. The latest title in the replay is the agent's current
 *  activity report. */
export function observeReplayedOutput({ id, data }: {
  id: string;
  data: string;
}) {
  observeContextScreen({
    id,
    text: data
  });
  observeAgentActivity({
    id,
    chunk: data
  });
}
