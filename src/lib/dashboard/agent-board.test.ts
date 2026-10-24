import {
  agentState,
  AgentState,
  BoardMessageKind,
  BoardSort,
  boardTally,
  changedFiles,
  elsewhereSessions,
  isDashboardOpen,
  isSummaryDue,
  orderedWindows,
  parseBoardMessage,
  windowFacts,
  windowsAfterMessage,
  windowsStillOpen
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
    project: "C:\\pade",
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
  const idle = window({
    projectLabel: "alpha",
    agents: [agent({ statusSince: NOW - MINUTE_MS })]
  });
  const busy = window({
    projectLabel: "zulu",
    agents: [agent({ status: SessionStatus.enum.working })]
  });
  const waiting = window({
    projectLabel: "mike",
    agents: [agent({ statusSince: NOW - 3 * MINUTE_MS }), agent({
      isAwaitingChoice: true,
      statusSince: NOW - 2 * MINUTE_MS
    })]
  });
  const quiet = window({
    projectLabel: "bravo",
    agents: [agent({ statusSince: NOW - 9 * MINUTE_MS })]
  });
  const windows = [idle, busy, quiet, waiting];
  function labels(sort: BoardSort) {
    return orderedWindows({
      windows,
      order: ["zulu", "alpha"],
      sort
    }).map(shown => shown.projectLabel);
  }

  it("keeps the user's window order whatever the agents are doing, then names the rest", () => {
    expect(labels(BoardSort.enum.windows)).toEqual(["zulu", "alpha", "bravo", "mike"]);
  });

  it("sorts by name", () => {
    expect(labels(BoardSort.enum.name)).toEqual(["alpha", "bravo", "mike", "zulu"]);
  });

  it("puts the window needing the user first, then the busy ones", () => {
    expect(labels(BoardSort.enum.attention)).toEqual(["mike", "zulu", "alpha", "bravo"]);
  });

  it("puts the most recently active window first", () => {
    expect(labels(BoardSort.enum.activity)).toEqual(["zulu", "alpha", "mike", "bravo"]);
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

  it("accepts a roll call", () => {
    expect(parseBoardMessage({ kind: BoardMessageKind.enum.rollCall }).success).toBe(true);
  });
});

describe("the board's windows", () => {
  const first = window({
    projectLabel: "w-1",
    agents: [agent({ id: "a" })]
  });
  const second = window({
    projectLabel: "w-2",
    agents: [agent({
      id: "b",
      project: "C:\\poll",
      conversationId: "conversation-b"
    })]
  });

  it("keeps each window's latest snapshot", () => {
    const withFirst = windowsAfterMessage({
      windows: new Map(),
      message: {
        kind: BoardMessageKind.enum.window,
        window: first
      }
    });
    const updated = windowsAfterMessage({
      windows: withFirst,
      message: {
        kind: BoardMessageKind.enum.window,
        window: {
          ...first,
          agents: []
        }
      }
    });
    expect(updated.get("w-1")?.agents).toEqual([]);
  });

  it("drops a window that said goodbye, so other rails stop showing its sessions live", () => {
    const windows = new Map([["w-1", first], ["w-2", second]]);
    const after = windowsAfterMessage({
      windows,
      message: {
        kind: BoardMessageKind.enum.windowGone,
        label: "w-2"
      }
    });
    expect([...after.keys()]).toEqual(["w-1"]);
    expect(
      elsewhereSessions({
        windows: after,
        ownLabel: "w-1"
      })
    ).toEqual([]);
  });

  it("drops a window that closed without a goodbye once it is no longer open", () => {
    const windows = new Map([["w-1", first], ["w-2", second]]);
    expect([
      ...windowsStillOpen({
        windows,
        openLabels: ["w-1"]
      }).keys()
    ]).toEqual(["w-1"]);
    expect(
      windowsStillOpen({
        windows,
        openLabels: ["w-1", "w-2"]
      })
    ).toBe(windows);
  });

  it("changes nothing on a message that carries no snapshot", () => {
    const windows = new Map([["w-1", first]]);
    expect(
      windowsAfterMessage({
        windows,
        message: {
          kind: BoardMessageKind.enum.rollCall
        }
      })
    ).toBe(windows);
  });

  it("lists the other windows' sessions with the window each runs in", () => {
    const sessions = elsewhereSessions({
      windows: new Map([["w-1", first], ["w-2", second]]),
      ownLabel: "w-1"
    });
    expect(sessions.map(session => [session.id, session.windowLabel, session.project])).toEqual([
      ["b", "w-2", "C:\\poll"]
    ]);
  });
});
