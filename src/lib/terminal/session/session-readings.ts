// What a session's agent records about itself on disk - its context fill and the
// model it is running - read back for the pane's gauges.

import { Command, EventType, onMessage, sendMessage } from "@/lib/ipc/messaging";
import { observeRunningModel, observeSessionLog } from "@/lib/stores/agent/context.svelte";
import { ensureModelOptions } from "@/lib/stores/model/modelOptions.svelte";
import type { AgentSession } from "@/lib/types";
import { coalesced } from "@/lib/util/async/coalesce";

export function createSessionReadings(session: () => AgentSession) {
  // Read the model and effort the agent itself reported (see running_model.rs) -
  // at mount, when a turn ends, and the moment the backend says a report or a
  // settings file changed. Coalesced so a switch reported mid-read still lands.
  const readRunningModel = coalesced(async () => {
    const { id, agent, conversationId } = session();
    if (conversationId === undefined) {
      return;
    }

    try {
      const running = await sendMessage(Command.agentRunningModel, {
        command: agent.command,
        conversationId
      });
      if (running === null) {
        return;
      }

      observeRunningModel({
        id,
        model: running.model,
        effort: running.effort,
        isUltracode: running.isUltracode
      });
      await ensureModelOptions(agent.command);
    } catch (error) {
      // The tag keeps what it showed; the next report or turn reads again.
      console.warn("pade: could not read the running model", error);
    }
  });

  return {
    // Read how full the context window is from the agent's own session log. Both
    // halves come off disk - the newest turn's token accounting, and the window its
    // recorded model advertises - so the reading holds no matter what the terminal
    // is or isn't painting. On any failure the store simply keeps what it had (a
    // guessed number could end a session wrongly); the gauge reads "measuring…" or
    // falls back to the screen signals, never a wrong number.
    //
    // This has to be RE-READ every turn, not fired once at mount. The fill grows
    // all session long, and the log carries no `"model"` field (nor any usage)
    // until the first assistant turn has been written - so a mount-time attempt
    // always lands too early on a fresh session and would leave
    // `measuredContextPercentage`, the one signal auto-handoff may act on, null for
    // the rest of that session's life.
    readContextFromSessionLog: coalesced(async () => {
      const { id, agent, conversationId } = session();
      try {
        const context = await sendMessage(Command.agentSessionContext, {
          command: agent.command,
          conversationId
        });
        if (context === null) {
          return;
        }

        observeSessionLog({
          id,
          usedTokens: context.usedTokens,
          windowTokens: context.windowTokens
        });
      } catch (error) {
        console.warn("pade: could not read the session's context fill", error);
      }
    }),
    readRunningModel,
    /** Re-read the moment the backend says this session's report - or a settings
     *  file every session reads - changed. */
    listenForModelChanges() {
      return onMessage(EventType.agentModelChanged, async ({ data: { sessionId } }) => {
        const isAboutThisSession = sessionId === null || sessionId === session().conversationId;
        if (isAboutThisSession) {
          await readRunningModel();
        }
      });
    }
  };
}
