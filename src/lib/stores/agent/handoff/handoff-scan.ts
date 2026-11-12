import { agentReportsBusy } from "@/lib/stores/agent/agentActivity.svelte";
import { compactionHeadroom, measuredContextPercentage } from "@/lib/stores/agent/context.svelte";
import { hasPersistentMarker } from "@/lib/stores/agent/handoff/handoff-markers";
import { isModelSwitchApplying } from "@/lib/stores/model/modelSwitch.svelte";
import { sessionStatus } from "@/lib/stores/session/sessions.svelte";
import { SessionStatus } from "@/lib/types/agent";
import type { AgentSession } from "@/lib/types/agent";

// The agent's own countdown to compaction (percent left) at which the handoff
// stops waiting for the turn to end. Claude counts down a percent every few
// thousand tokens, and writing the doc itself takes a turn's worth of room.
const COMPACTION_IMMINENT_PERCENTAGE = 5;

/** What the near-limit scan does with one session. */
export const ScanVerdict = {
  Leave: "leave",
  HoldBack: "hold-back",
  HandOff: "hand-off",
  HandOffMidTurn: "hand-off-mid-turn"
} as const;
type ScanVerdict = (typeof ScanVerdict)[keyof typeof ScanVerdict];

function isOverThreshold({ sessionId, thresholdPercentage }: {
  sessionId: string;
  thresholdPercentage: number;
}) {
  const percentage = measuredContextPercentage(sessionId);
  return percentage !== null && percentage >= thresholdPercentage;
}

function isAboutToCompact(sessionId: string) {
  const headroom = compactionHeadroom(sessionId);
  return headroom !== null && headroom <= COMPACTION_IMMINENT_PERCENTAGE;
}

// A persistent marker with no live handoff is one whose async flow died
// between marking and the kill/launch - a window reload, an HMR swap, or a
// crash mid-handoff. It leaves the session stranded: the doc is written but
// the id never reset, and the old code skipped it forever on that very
// marker. Resume instead - runHandoff's already-written probe short-circuits
// onto the doc it already wrote, straight to the reset. A session with no
// marker starts a fresh handoff only once it is over threshold.
function isHandoffWanted({ sessionId, thresholdPercentage }: {
  sessionId: string;
  thresholdPercentage: number;
}) {
  const isResuming = hasPersistentMarker(sessionId);
  const isNearLimit = isOverThreshold({
    sessionId,
    thresholdPercentage
  });
  return isResuming || isNearLimit;
}

/** What the scan does with one session: leave it, hold it back for the agent's
 *  running work, or start its handoff. `isHandingOffHere` is whether a handoff
 *  actively running in THIS module instance already owns the session. */
export function scanVerdict({ session, isHandingOffHere, thresholdPercentage }: {
  session: AgentSession;
  isHandingOffHere: boolean;
  thresholdPercentage: number;
}) {
  if (isHandingOffHere) {
    return ScanVerdict.Leave;
  }

  const isWanted = isHandoffWanted({
    sessionId: session.id,
    thresholdPercentage
  });
  const isCompactionImminent = isAboutToCompact(session.id);
  if (!isWanted && !isCompactionImminent) {
    return ScanVerdict.Leave;
  }

  // A model switch being applied always goes first: both type into the
  // session, and a switch that relaunches it would end the very session being
  // handed off. The scan comes back to the switched session.
  if (isModelSwitchApplying(session.id)) {
    return ScanVerdict.HoldBack;
  }

  if (isCompactionImminent) {
    return ScanVerdict.HandOffMidTurn;
  }

  // Otherwise never cut in on the agent's own running work: a turn, or
  // background agents and a dynamic workflow a quiet, `ready`-looking session
  // is still waiting on. The agent reporting it done re-runs the scan.
  const isQuiet = !agentReportsBusy(session.id)
    && sessionStatus(session.id) === SessionStatus.enum.ready;
  return isQuiet ? ScanVerdict.HandOff : ScanVerdict.HoldBack;
}
