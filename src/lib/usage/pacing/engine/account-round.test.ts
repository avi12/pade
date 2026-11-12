import { SubagentStepKind } from "@/lib/types/pacing";
import {
  controlThrottle,
  freshReports,
  isThrottling,
  PacingMessageKind,
  parsePacingMessage,
  planRound,
  roundLeader,
  subagentKey
} from "@/lib/usage/pacing/engine/account-round";
import type { SessionReport, SubagentSteps, WindowReport } from "@/lib/usage/pacing/engine/account-round";
import { holdForSaving, MAXIMUM_HOLD_SECONDS, NO_SPAWN_SHED } from "@/lib/usage/pacing/engine/subagent-steps";
import { describe, expect, it } from "vitest";

const NOW = Date.UTC(2026, 10, 7, 12);
const ACCOUNT = "avi@example.com";
const NO_STEPS: SubagentSteps = {};
const NEVER_STEPPED: ReadonlyMap<string, number> = new Map();

function session(overrides: Partial<SessionReport> & Pick<SessionReport, "id">): SessionReport {
  return {
    name: overrides.id,
    accountKey: ACCOUNT,
    command: "claude",
    conversationId: `${overrides.id}-conversation`,
    reportedAt: NOW,
    load: 10,
    overPace: null,
    canStepUp: false,
    hasRoom: false,
    subagents: [],
    ...overrides
  };
}

function windowReport(label: string, sessions: SessionReport[]): WindowReport {
  return {
    label,
    at: NOW,
    sessions
  };
}

function round(reports: WindowReport[], steps = NO_STEPS, steppedAt = NEVER_STEPPED) {
  return planRound({
    reports,
    steps,
    steppedAt,
    leader: "main",
    now: NOW
  });
}

describe("the round's leader", () => {
  it("is one window, the same whichever window asks", () => {
    const reports = [windowReport("window-2", []), windowReport("main", []), windowReport("window-1", [])];
    expect(roundLeader(reports)).toBe("main");
    expect(roundLeader(reports.toReversed())).toBe("main");
    expect(roundLeader([])).toBeNull();
  });

  it("drops a window that stopped reporting, so the next one leads", () => {
    const reports = [{
      ...windowReport("main", []),
      at: NOW - 10 * 60_000
    }, windowReport("window-1", [])];
    const fresh = freshReports({
      reports,
      now: NOW,
      lifetimeMs: 150_000
    });
    expect(roundLeader(fresh)).toBe("window-1");
  });
});

describe("planRound across windows sharing one account", () => {
  it("steps the biggest saver wherever it lives, not the leader's own session first", () => {
    const front = session({
      id: "front",
      load: 10,
      overPace: {
        shed: 0.1,
        saving: 0.2
      }
    });
    const background = session({
      id: "background",
      load: 40,
      overPace: {
        shed: 0.1,
        saving: 0.2
      }
    });
    const { plan } = round([windowReport("main", [front]), windowReport("window-1", [background])]);
    expect(plan.climb).toEqual(["background"]);
    expect(plan.steppedAccounts).toEqual([ACCOUNT]);
  });

  it("sheds the account's fraction of every window's load once, not once per window", () => {
    const sessions = ["a", "b", "c", "d"].map(id => session({
      id,
      load: 25,
      overPace: {
        shed: 0.2,
        saving: 0.5
      }
    }));
    const { plan } = round([
      windowReport("main", sessions.slice(0, 2)),
      windowReport("window-1", sessions.slice(2))
    ]);
    // 20% of 100 needs 20 saved; each step saves 12.5, so two - not four.
    expect(plan.climb).toHaveLength(2);
  });

  it("ranks a background session's running subagent against the sessions, by its own load", () => {
    const front = session({
      id: "front",
      load: 5,
      overPace: {
        shed: 0.1,
        saving: 0.2
      }
    });
    const background = session({
      id: "background",
      load: 5,
      subagents: [{
        id: "a1",
        description: "Explore the repo",
        load: 60
      }]
    });
    const { plan, steps } = round([windowReport("main", [front]), windowReport("window-1", [background])]);
    const key = subagentKey({
      sessionId: "background",
      subagentId: "a1"
    });
    expect(steps[key]).toEqual({
      step: SubagentStepKind.enum.gate,
      holdSeconds: holdForSaving(0.1)
    });
    expect(plan.climb).toEqual([]);
    const throttle = plan.throttles.find(entry => entry.sessionId === "background");
    expect(throttle?.subagents.map(subagent => subagent.description)).toEqual(["Explore the repo"]);
    expect(throttle?.isBlockingSpawns).toBe(false);
  });

  it("wraps up a subagent already held the longest, and keeps the others' steps", () => {
    const key = subagentKey({
      sessionId: "s",
      subagentId: "a1"
    });
    const kept = subagentKey({
      sessionId: "s",
      subagentId: "a2"
    });
    const steps: SubagentSteps = {
      [key]: {
        step: SubagentStepKind.enum.gate,
        holdSeconds: MAXIMUM_HOLD_SECONDS
      },
      [kept]: {
        step: SubagentStepKind.enum.gate,
        holdSeconds: 10
      }
    };
    const sessions = [session({
      id: "s",
      load: 1,
      overPace: {
        shed: 0.1,
        saving: 0
      },
      subagents: [{
        id: "a1",
        description: null,
        load: 50
      }, {
        id: "a2",
        description: null,
        load: 1
      }]
    })];
    const { steps: next } = round([windowReport("window-1", sessions)], steps);
    expect(next[key]).toEqual({
      step: SubagentStepKind.enum.wrapUp
    });
    expect(next[kept]).toEqual(steps[kept]);
  });

  it("blocks new subagents on every session of the account only past the threshold", () => {
    function shedding(shed: number) {
      return round([
        windowReport("main", [session({
          id: "a",
          overPace: {
            shed,
            saving: 0.2
          }
        })]),
        windowReport("window-1", [session({ id: "b" })])
      ]).plan.throttles.map(throttle => throttle.isBlockingSpawns);
    }

    expect(shedding(NO_SPAWN_SHED)).toEqual([true, true]);
    expect(shedding(NO_SPAWN_SHED / 2)).toEqual([false, false]);
  });

  it("keeps other accounts out of the round", () => {
    const other = session({
      id: "other",
      accountKey: "someone@example.com",
      subagents: [{
        id: "a1",
        description: null,
        load: 90
      }]
    });
    const over = session({
      id: "over",
      overPace: {
        shed: 0.5,
        saving: 0.5
      }
    });
    const { plan, steps } = round([windowReport("main", [over]), windowReport("window-1", [other])]);
    expect(steps).toEqual({});
    expect(plan.throttles.find(entry => entry.sessionId === "other")?.isBlockingSpawns).toBe(false);
  });

  it("does not step a session again on a report older than its last step", () => {
    const stale = session({
      id: "s",
      reportedAt: NOW - 30_000,
      overPace: {
        shed: 0.5,
        saving: 0.5
      }
    });
    const { plan } = round([windowReport("window-1", [stale])], NO_STEPS, new Map([["s", NOW - 10_000]]));
    expect(plan.climb).toEqual([]);
  });

  it("with room again on every session, eases holds and restores the lightest stepped-down session", () => {
    const key = subagentKey({
      sessionId: "a",
      subagentId: "a1"
    });
    const steps: SubagentSteps = {
      [key]: {
        step: SubagentStepKind.enum.gate,
        holdSeconds: 20
      },
      [subagentKey({
        sessionId: "a",
        subagentId: "finished"
      })]: {
        step: SubagentStepKind.enum.wrapUp
      }
    };
    const reports = [
      windowReport("main", [session({
        id: "a",
        hasRoom: true,
        subagents: [{
          id: "a1",
          description: null,
          load: 5
        }]
      })]),
      windowReport("window-1", [session({
        id: "light",
        load: 2,
        hasRoom: true,
        canStepUp: true
      }), session({
        id: "heavy",
        load: 20,
        hasRoom: true,
        canStepUp: true
      })])
    ];
    const { plan, steps: next } = round(reports, steps);
    expect(next).toEqual({
      [key]: {
        step: SubagentStepKind.enum.gate,
        holdSeconds: 10
      }
    });
    expect(plan.restore).toEqual(["light"]);
  });

  it("holds the steps while some session on the account has no room yet", () => {
    const key = subagentKey({
      sessionId: "a",
      subagentId: "a1"
    });
    const steps: SubagentSteps = {
      [key]: {
        step: SubagentStepKind.enum.gate,
        holdSeconds: 20
      }
    };
    const { steps: next } = round([
      windowReport("main", [session({
        id: "a",
        hasRoom: true,
        subagents: [{
          id: "a1",
          description: null,
          load: 5
        }]
      })]),
      windowReport("window-1", [session({ id: "b" })])
    ], steps);
    expect(next).toEqual(steps);
  });
});

describe("the wire between windows", () => {
  it("accepts a well-formed plan and refuses a malformed one", () => {
    const { plan } = round([windowReport("main", [session({ id: "s" })])]);
    expect(
      parsePacingMessage({
        kind: PacingMessageKind.enum.plan,
        plan
      }).success
    ).toBe(true);
    expect(
      parsePacingMessage({
        kind: PacingMessageKind.enum.plan,
        plan: {
          leader: 1
        }
      }).success
    ).toBe(false);
  });

  it("writes a throttle as the control file holds it", () => {
    const throttle = {
      isBlockingSpawns: false,
      subagents: [{
        id: "a1",
        description: null,
        step: {
          step: SubagentStepKind.enum.wrapUp
        }
      }]
    };
    expect(isThrottling(throttle)).toBe(true);
    expect(controlThrottle(throttle)).toEqual({
      isBlockingSpawns: false,
      subagents: {
        a1: {
          step: SubagentStepKind.enum.wrapUp
        }
      }
    });
    expect(
      isThrottling({
        isBlockingSpawns: false,
        subagents: []
      })
    ).toBe(false);
  });
});
