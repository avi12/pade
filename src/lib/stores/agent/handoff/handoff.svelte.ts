// Auto-handoff: when an agent nears its context window, ask it to write a
// continue-<slug>.md handoff doc, end the session, and start a fresh successor
// seeded to resume from that doc. The successor is the SAME agent while it still
// has usage headroom (a context-driven handoff); when the current agent is
// tapped out, the handoff crosses over to the first other available agent that
// does (Claude→Codex, generalized to any agents). Opt-out via prefs.autoHandoff;
// fires once per session. This module owns the machinery - thresholds, successor
// selection, the settle-wait for the doc, resource teardown - while the app
// shell supplies its session list, its available-agent pool and launch through
// `HandoffHost` and drives the scan from a component `$effect`.
//
// A handoff ends the session, and with it everything the agent still has running
// in the background - a dynamic workflow, background agents. So no handoff starts,
// and none ends a session, while the agent itself reports work in flight
// (lib/stores/agent/agentActivity); it waits for that work to finish, however long.
// The one exception is an agent about to compact its own conversation: waiting
// for its turn to end would let the compaction run first and lose the detail a
// handoff exists to keep, so the request goes in mid-turn - the agent picks it
// up between steps - and the session ends once the doc is on disk.

import { handoff, writeHandoffDoc } from "@/lib/stores/agent/handoff/handoff-cycle";
import type { HandoffContext } from "@/lib/stores/agent/handoff/handoff-doc-request";
import type { HandoffHost } from "@/lib/stores/agent/handoff/handoff-host";
import type { HandoffReason } from "@/lib/stores/agent/handoff/handoff-prompts";
import { HandoffState } from "@/lib/stores/agent/handoff/handoff-state.svelte";
import { createHandoffTimers } from "@/lib/stores/agent/handoff/handoff-timers";
import { forceHandoff, restartForConfiguration, scanForHandoffs } from "@/lib/stores/agent/handoff/handoff-trigger";
import type { AgentSession } from "@/lib/types/agent";

/** The auto-handoff machinery, scoped to one app shell. The shell calls
 *  `check()` from a `$effect` and `dispose()` on destroy; `note` is the status
 *  line to show while a handoff is in flight ("" when idle). */
export function createAutoHandoff(host: HandoffHost) {
  const state = new HandoffState();
  const timers = createHandoffTimers();
  const context: HandoffContext = {
    host,
    state,
    timers,
    runHandoff
  };

  // Fire-and-forget entry point for the scan and the force path. handoff is
  // best-effort: swallow any failure (including deleting a doc the agent never
  // wrote on the timeout path) and clear the in-flight marker + note so a later
  // scan can retry.
  async function runHandoff({ session, reason, isMidTurn = false }: {
    session: AgentSession;
    reason: HandoffReason;
    isMidTurn?: boolean;
  }) {
    try {
      await handoff({
        context,
        session,
        reason,
        isMidTurn
      });
    } catch (error) {
      console.warn(`pade: handoff of ${session.agent.label} failed, a later scan retries`, error);
      state.abandonHandoff(session.id);
    }
  }

  return {
    /** Status line shown while a handoff is in flight or held back for the agent's
     *  running work ("" when idle). */
    get note() {
      return state.note || state.deferredNote;
    },
    check: () => scanForHandoffs(context),
    force: (session: AgentSession) => forceHandoff({
      context,
      session
    }),
    isHandingOff: (sessionId: string) => state.isHandingOff(sessionId),
    restartForConfiguration: (session: AgentSession) => restartForConfiguration({
      context,
      session
    }),
    writeHandoffDoc: (session: AgentSession) => writeHandoffDoc({
      context,
      session
    }),
    dispose: timers.dispose
  };
}
