import {
  agentState,
  AgentState,
  boardTally,
  changedFiles,
  isDashboardOpen,
  isSummaryDue,
  orderedWindows,
  parseBoardMessage,
  windowFacts
} from "@/lib/dashboard/agent-board";
import type { BoardAgent, BoardWindow } from "@/lib/dashboard/agent-board";
import { SessionStatus } from "@/lib/types";
import { describe, expect, it } from "vitest";

const NOW = 1_000_000;
const MINUTE_MS = 60_000;

function agent(overrides: Partial<BoardAgent> = {}) {
  return {
    id: "s-1",
    agentId: "claude",
    agentLabel: "Claude",
    name: "fix-toast",
    status: SessionStatus.enum.ready,
    isAwaitingChoice: false,
    model: null,
    summary: null,
    summarizedAt: null,
    statusSince: NOW,
    ...overrides
  };
}

function window({ projectLabel, agents }: {
  projectLabel: string;
  agents: BoardAgent[];
}): BoardWindow {
  return {
    label: projectLabel,
    project: `C:\\${projectLabel}`,
    projectLabel,
    agents,
    changedFiles: [],
    changedFileCount: 0,
    lastChangeAt: null
  };
}

describe("agentState", () => {
  it("puts a pending choice above whatever the status says", () => {
    expect(
      agentState(
        agent({
          status: SessionStatus.enum.working,
          isAwaitingChoice: true
        })
      )
    ).toBe(AgentState.Waiting);
    expect(agentState(agent({ status: SessionStatus.enum.working }))).toBe(AgentState.Working);
  });
});

describe("orderedWindows", () => {
  it("shows the window needing the user first, then the busy ones, then by name", () => {
    const idle = window({
      projectLabel: "alpha",
      agents: [agent()]
    });
    const busy = window({
      projectLabel: "zulu",
      agents: [agent({ status: SessionStatus.enum.working })]
    });
    const waiting = window({
      projectLabel: "mike",
      agents: [agent(), agent({ isAwaitingChoice: true })]
    });
    const quiet = window({
      projectLabel: "bravo",
      agents: [agent()]
    });
    expect(orderedWindows([idle, busy, quiet, waiting]).map(shown => shown.projectLabel))
      .toEqual(["mike", "zulu", "alpha", "bravo"]);
  });
});

describe("boardTally", () => {
  it("counts every window's agents by state, most urgent first, skipping empty ones", () => {
    expect(
      boardTally([
        window({
          projectLabel: "a",
          agents: [agent({ status: SessionStatus.enum.working }), agent(), agent()]
        }),
        window({
          projectLabel: "b",
          agents: [agent({ isAwaitingChoice: true })]
        })
      ])
    ).toEqual([
      {
        state: AgentState.Waiting,
        count: 1
      },
      {
        state: AgentState.Working,
        count: 1
      },
      {
        state: AgentState.Ready,
        count: 2
      }
    ]);
  });
});

describe("windowFacts", () => {
  it("says how many agents and changed files, and when the last change landed", () => {
    const board = window({
      projectLabel: "a",
      agents: [agent()]
    });
    expect(
      windowFacts({
        board,
        now: NOW
      })
    ).toBe("1 agent · 0 files changed");
    expect(
      windowFacts({
        board: {
          ...board,
          changedFileCount: 3,
          lastChangeAt: NOW - 2 * MINUTE_MS
        },
        now: NOW
      })
    ).toBe("1 agent · 3 files changed · last 2m ago");
  });
});

describe("changedFiles", () => {
  it("names each changed file once, newest first", () => {
    expect(changedFiles([{ path: "b" }, { path: "a" }, { path: "b" }])).toEqual({
      recent: ["b", "a"],
      count: 2
    });
  });
});

describe("isDashboardOpen", () => {
  it("counts the dashboard open only while it keeps announcing itself", () => {
    expect(
      isDashboardOpen({
        seenAt: NOW - MINUTE_MS,
        now: NOW
      })
    ).toBe(true);
    expect(
      isDashboardOpen({
        seenAt: NOW - 5 * MINUTE_MS,
        now: NOW
      })
    ).toBe(false);
    expect(
      isDashboardOpen({
        seenAt: null,
        now: NOW
      })
    ).toBe(false);
  });
});

describe("isSummaryDue", () => {
  it("summarizes a working or idle agent the first time", () => {
    expect(
      isSummaryDue({
        status: SessionStatus.enum.working,
        statusSince: NOW,
        summarizedAt: null,
        now: NOW
      })
    ).toBe(true);
    expect(
      isSummaryDue({
        status: SessionStatus.enum.starting,
        statusSince: NOW,
        summarizedAt: null,
        now: NOW
      })
    ).toBe(false);
  });

  it("re-summarizes once an agent stops after its last summary", () => {
    expect(
      isSummaryDue({
        status: SessionStatus.enum.ready,
        statusSince: NOW - MINUTE_MS,
        summarizedAt: NOW - 2 * MINUTE_MS,
        now: NOW
      })
    ).toBe(true);
    expect(
      isSummaryDue({
        status: SessionStatus.enum.ready,
        statusSince: NOW - 3 * MINUTE_MS,
        summarizedAt: NOW - 2 * MINUTE_MS,
        now: NOW
      })
    ).toBe(false);
  });

  it("refreshes a working agent every couple of minutes, never faster", () => {
    expect(
      isSummaryDue({
        status: SessionStatus.enum.working,
        statusSince: NOW - 10 * MINUTE_MS,
        summarizedAt: NOW - 3 * MINUTE_MS,
        now: NOW
      })
    ).toBe(true);
    expect(
      isSummaryDue({
        status: SessionStatus.enum.working,
        statusSince: NOW - 10 * MINUTE_MS,
        summarizedAt: NOW - MINUTE_MS,
        now: NOW
      })
    ).toBe(false);
  });
});

describe("parseBoardMessage", () => {
  it("accepts a well-formed message and refuses a stray one", () => {
    expect(
      parseBoardMessage({
        kind: "focus",
        label: "w-1",
        sessionId: "s-1"
      }).success
    ).toBe(true);
    expect(parseBoardMessage({ kind: "focus" }).success).toBe(false);
  });
});
