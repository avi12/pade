// What starts a handoff: the near-limit scan, a forced handoff and a
// configuration restart. Each marks the session as mid-handoff and hands it to
// the context's `runHandoff`.

import type { HandoffContext } from "@/lib/stores/agent/handoff/handoff-doc-request";
import { waitingForWorkNote } from "@/lib/stores/agent/handoff/handoff-notes";
import { HandoffReason } from "@/lib/stores/agent/handoff/handoff-prompts";
import { ScanVerdict, scanVerdict } from "@/lib/stores/agent/handoff/handoff-scan";
import type { AgentSession } from "@/lib/types/agent";

// Scan for sessions to hand off: those newly over the context threshold, and any
// whose earlier handoff was interrupted before it could reset.
export function scanForHandoffs(context: HandoffContext) {
  const { host, state } = context;
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
    if (verdict === ScanVerdict.HOLD_BACK) {
      heldBackNote ||= waitingForWorkNote({
        session,
        reason: HandoffReason.CONTEXT_LIMIT
      });
      continue;
    }

    const isHandOff = verdict === ScanVerdict.HAND_OFF || verdict === ScanVerdict.HAND_OFF_MID_TURN;
    if (isHandOff) {
      state.markHandingOff(session.id);
      void context.runHandoff({
        session,
        reason: HandoffReason.CONTEXT_LIMIT,
        isMidTurn: verdict === ScanVerdict.HAND_OFF_MID_TURN
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
export function forceHandoff({ context, session }: { context: HandoffContext; session: AgentSession }) {
  if (context.state.isHandingOff(session.id)) {
    return;
  }

  context.state.markHandingOff(session.id);
  void context.runHandoff({
    session,
    reason: HandoffReason.CONTEXT_LIMIT
  });
}

// An MCP membership change must launch the same governed agent again so it
// reads the new configuration. Unlike a context-limit handoff, this bypasses
// quota-based crossover while retaining the document-first safety sequence.
export async function restartForConfiguration({
  context,
  session
}: {
  context: HandoffContext;
  session: AgentSession;
}) {
  if (context.state.isHandingOff(session.id)) {
    return;
  }

  context.state.markHandingOff(session.id);
  await context.runHandoff({
    session,
    reason: HandoffReason.CONFIGURATION_CHANGE
  });
}
