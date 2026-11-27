import { describe, expect, it, vi } from "vitest";

import { scanVerdict, ScanVerdict } from "@/lib/stores/agent/handoff/handoff-scan";
import { SessionStatus } from "@/lib/types/agent";
import type { AgentSession } from "@/lib/types/agent";

const facts = vi.hoisted(() => ({
  percentage: 65 as number | null,
  isBusy: true,
  isGoalActive: false
}));

vi.mock("@/lib/stores/agent/context.svelte", () => ({
  measuredContextPercentage: () => facts.percentage,
  compactionHeadroom: () => null
}));
vi.mock("@/lib/stores/agent/agentActivity.svelte", () => ({
  agentReportsBusy: () => facts.isBusy
}));
vi.mock("@/lib/stores/agent/sessionGoals.svelte", () => ({
  isGoalActive: () => facts.isGoalActive
}));
vi.mock("@/lib/stores/agent/handoff/handoff-markers", () => ({
  hasPersistentMarker: () => false
}));
vi.mock("@/lib/stores/model/modelSwitch.svelte", () => ({
  isModelSwitchApplying: () => false
}));
vi.mock("@/lib/stores/session/sessions.svelte", () => ({
  sessionStatus: () => (facts.isBusy ? SessionStatus.enum.working : SessionStatus.enum.ready)
}));

const SESSION = { id: "goal-session" } as AgentSession;
const THRESHOLD = 60;

function verdict() {
  return scanVerdict({
    session: SESSION,
    isHandingOffHere: false,
    thresholdPercentage: THRESHOLD
  });
}

describe("scanVerdict", () => {
  it("holds a busy session past the threshold back for its running work", () => {
    facts.isGoalActive = false;
    expect(verdict()).toBe(ScanVerdict.HOLD_BACK);
  });

  it("hands a session with a running goal off mid-turn - the goal never lets it go quiet", () => {
    facts.isGoalActive = true;
    expect(verdict()).toBe(ScanVerdict.HAND_OFF_MID_TURN);
  });

  it("leaves a session with a goal alone under the threshold", () => {
    facts.isGoalActive = true;
    facts.percentage = 40;
    expect(verdict()).toBe(ScanVerdict.LEAVE);
    facts.percentage = 65;
  });
});
