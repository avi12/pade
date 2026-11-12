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

import { docNameFor, handoff } from "@/lib/stores/agent/handoff/handoff-cycle";
import { askForDoc } from "@/lib/stores/agent/handoff/handoff-doc-request";
import type { HandoffContext } from "@/lib/stores/agent/handoff/handoff-doc-request";
import type { HandoffHost } from "@/lib/stores/agent/handoff/handoff-host";
import { waitingForWorkNote } from "@/lib/stores/agent/handoff/handoff-notes";
import { HandoffReason } from "@/lib/stores/agent/handoff/handoff-prompts";
import { ScanVerdict, scanVerdict } from "@/lib/stores/agent/handoff/handoff-scan";
import { HandoffState } from "@/lib/stores/agent/handoff/handoff-state.svelte";
import { createHandoffTimers } from "@/lib/stores/agent/handoff/handoff-timers";
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

  // Ask `session`'s agent to write its handoff doc and resolve once the doc is on
  // disk - the reusable core of the handoff, WITHOUT the kill/launch. The
  // workspace-save flow uses it: the doc is written into the temp workspace, rides
  // along in the copy to the saved project, and the successor there is seeded to
  // continue from it. Returns the doc's file name, or null when the agent never
  // produced it (the caller falls back to a plain restart). A doc a prior attempt
  // already wrote is reused, not re-requested.
  async function writeHandoffDoc(session: AgentSession) {
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

  // Scan for sessions to hand off: those newly over the context threshold, and any
  // whose earlier handoff was interrupted before it could reset.
  function check() {
    if (host.isOptedOut()) {
      state.deferredNote = "";
      return;
    }

    let heldBackNote = "";
    for (const session of host.sessions()) {
      const verdict = scanVerdict({
        session,
        isHandingOffHere: state.handingOff.has(session.id),
        thresholdPercentage: host.thresholdPercentage()
      });
      if (verdict === ScanVerdict.HoldBack) {
        heldBackNote ||= waitingForWorkNote({
          session,
          reason: HandoffReason.ContextLimit
        });
        continue;
      }

      if (verdict === ScanVerdict.HandOff || verdict === ScanVerdict.HandOffMidTurn) {
        state.markHandingOff(session.id);
        runHandoff({
          session,
          reason: HandoffReason.ContextLimit,
          isMidTurn: verdict === ScanVerdict.HandOffMidTurn
        });
      }
    }

    state.deferredNote = heldBackNote;
  }

  // Hand a session off right now - the usage-resume flow calls this at window
  // reset when the context is too full to just continue. Same single-flight
  // guard as the scan, none of its idle/threshold gates: the caller has
  // already decided this session must cycle. It still waits for the agent's
  // running work to finish before asking for the doc (see handoff).
  function force(session: AgentSession) {
    if (state.isHandingOff(session.id)) {
      return;
    }

    state.markHandingOff(session.id);
    runHandoff({
      session,
      reason: HandoffReason.ContextLimit
    });
  }

  // An MCP membership change must launch the same governed agent again so it
  // reads the new configuration. Unlike a context-limit handoff, this bypasses
  // quota-based crossover while retaining the document-first safety sequence.
  async function restartForConfiguration(session: AgentSession) {
    if (state.isHandingOff(session.id)) {
      return;
    }

    state.markHandingOff(session.id);
    await runHandoff({
      session,
      reason: HandoffReason.ConfigurationChange
    });
  }

  return {
    /** Status line shown while a handoff is in flight or held back for the agent's
     *  running work ("" when idle). */
    get note() {
      return state.note || state.deferredNote;
    },
    check,
    force,
    isHandingOff: (sessionId: string) => state.isHandingOff(sessionId),
    restartForConfiguration,
    writeHandoffDoc,
    dispose: timers.dispose
  };
}
