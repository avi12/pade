import type { ModelOption } from "@/lib/types/agent";
import { PacedWindowKind } from "@/lib/types/pacing";
import { costWeight } from "@/lib/usage/pacing/engine/load-shedding";
import { PacePhase, PacingReport } from "@/lib/usage/pacing/pacing-report";
import type { SessionFacts } from "@/lib/usage/pacing/pacing-report";
import { choiceFactsOf, paceFactsOf, sessionFactsOf } from "@/lib/usage/pacing/report-facts";
import { describe, expect, it } from "vitest";

function option({ id, efforts }: {
  id: string;
  efforts: string[];
}) {
  return {
    id,
    name: id,
    efforts: efforts.map(level => ({
      id: level,
      name: level
    })),
    recommendedEffort: null
  } satisfies ModelOption;
}

const OPTIONS = [
  option({
    id: "claude-opus-5-5",
    efforts: ["low", "medium", "high"]
  }),
  option({
    id: "claude-sonnet-5-5",
    efforts: ["low", "medium", "high"]
  }),
  option({
    id: "claude-haiku-4-5-20251001",
    efforts: []
  })
];

describe("choiceFactsOf", () => {
  it("is null before the agent has reported what it runs", () => {
    expect(
      choiceFactsOf({
        running: null,
        original: null,
        restoreAt: null,
        isSteppedDown: false,
        options: OPTIONS
      })
    ).toBeNull();
  });

  it("says which steps are left and what each would save of the session's own load", () => {
    const facts = choiceFactsOf({
      running: {
        id: "claude-opus-5-5",
        effort: "high"
      },
      original: null,
      restoreAt: null,
      isSteppedDown: false,
      options: OPTIONS
    });
    expect(facts?.canStepDown).toEqual({
      effort: true,
      model: true
    });
    expect(facts?.canStepUp).toBe(false);
    expect(facts?.effortSaving).toBeCloseTo(
      1 - costWeight({
        model: "claude-opus-5-5",
        effort: "medium"
      }) / costWeight({
        model: "claude-opus-5-5",
        effort: "high"
      })
    );
    expect(facts?.effortSaving).toBeGreaterThan(0);
    expect(facts?.modelSaving).toBeGreaterThan(0);
  });

  it("has nothing to save at the bottom of both ladders, and can step up once stepped down", () => {
    const facts = choiceFactsOf({
      running: {
        id: "claude-haiku-4-5-20251001",
        effort: null
      },
      original: {
        model: "claude-opus-5-5",
        effort: "high"
      },
      restoreAt: 5000,
      isSteppedDown: true,
      options: OPTIONS
    });
    expect(facts).toMatchObject({
      model: "claude-haiku-4-5-20251001",
      effort: null,
      original: {
        model: "claude-opus-5-5",
        effort: "high"
      },
      canStepDown: {
        effort: false,
        model: false
      },
      canStepUp: true,
      restoreAt: 5000,
      effortSaving: 0,
      modelSaving: 0
    });
  });
});

describe("paceFactsOf", () => {
  const state = {
    phase: PacePhase.enum.paused,
    windowKind: PacedWindowKind.enum.session,
    readyAt: 2,
    continueAt: 3,
    restUntil: null,
    hasStoppedWork: true,
    isHeld: false,
    pausedAt: 1,
    rates: {
      five_hour: 1
    }
  };

  it("is null for a session that is not paused", () => {
    expect(paceFactsOf(undefined)).toBeNull();
  });

  it("carries only what the engine reads, leaving the loop's own bookkeeping behind", () => {
    const withBookkeeping = {
      ...state,
      utilization: 80
    };
    expect(paceFactsOf(withBookkeeping)).toEqual(state);
  });
});

describe("sessionFactsOf", () => {
  const reported = {
    id: "s1",
    conversationId: null,
    agentId: "claude",
    command: "claude",
    name: "Claude",
    backgroundAgentCount: 0,
    isPacingOn: true,
    isHandingOff: false,
    hasUsageLimit: false,
    isModelSwitchApplying: false,
    choice: null
  } satisfies Omit<SessionFacts, "isWorking" | "isSkipped" | "isContinuedRun" | "isShielded" | "workingSince" | "workingShare" | "ownLoad" | "subagents" | "pace">;

  it("joins what the app reports with what the loop owns into a valid wire report", () => {
    const session = sessionFactsOf({
      reported,
      owned: {
        isWorking: true,
        isSkipped: false,
        isContinuedRun: false,
        isShielded: false,
        workingSince: 1000,
        workingShare: 0.5,
        ownLoad: 2,
        subagents: [{
          id: "a",
          description: null,
          load: 1
        }],
        pace: null
      }
    });
    const report = {
      windowLabel: "main",
      at: 5000,
      sessions: [session],
      observed: [{
        sessionId: "s1",
        at: 5000,
        kind: "stepUp"
      }]
    };
    expect(PacingReport.safeParse(report).success).toBe(true);
    expect(session.isWorking).toBe(true);
    expect(session.id).toBe("s1");
  });
});
