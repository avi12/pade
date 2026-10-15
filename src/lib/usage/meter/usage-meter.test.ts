import { type AccountUsage, UsageWindowKind } from "@/lib/types";
import {
  groupSeverityClass,
  hasClaudePill,
  isJustReset,
  KEEP_LAST_READING_MS,
  measuredAgentCount,
  overflowCount,
  overflowLevel,
  pendingResetBoundaries,
  severityClass,
  triggerPillGroups,
  USAGE_PANEL_LABEL,
  usageAriaLabel,
  withLastGoodReadings
} from "@/lib/usage/meter/usage-meter";
import { type AgentGroup, Level, type Limit, limitKey } from "@/lib/usage/usage-groups";
import { describe, expect, it } from "vitest";

function makeLimit({ label, level }: {
  label: string;
  level: Level;
}) {
  return {
    label,
    sub: "",
    reset: "",
    resetAt: "",
    percentage: 50,
    reading: "50%",
    level,
    kind: UsageWindowKind.enum.session,
    kindShort: "S",
    balance: "",
    isCarryingWork: false,
    isUncapped: false
  } satisfies Limit;
}

function makeGroup({ id, levels }: {
  id: string;
  levels: Level[];
}) {
  return {
    id,
    name: id,
    shortName: id,
    plan: "",
    icon: "bolt",
    isUnknown: levels.length === 0,
    limits: levels.map((level, i) => makeLimit({
      label: `limit ${i}`,
      level
    }))
  } satisfies AgentGroup;
}

function makeAccount(resets: (string | null)[]) {
  return {
    windows: resets.map((resetsAt, i) => ({
      key: `window-${i}`,
      kind: UsageWindowKind.enum.session,
      label: "Session",
      utilization: 10,
      resetsAt
    })),
    plan: "",
    source: ""
  } satisfies AccountUsage;
}

describe("severityClass", () => {
  it("paints warning and critical, and leaves normal and unknown bare", () => {
    expect(severityClass(Level.Warning)).toBe("warning");
    expect(severityClass(Level.Critical)).toBe("critical");
    expect(severityClass(Level.Normal)).toBeUndefined();
    expect(severityClass(null)).toBeUndefined();
  });
});

describe("trigger pills and overflow", () => {
  const groups = [
    makeGroup({
      id: "claude",
      levels: [Level.Normal]
    }),
    makeGroup({
      id: "codex",
      levels: [Level.Normal]
    }),
    makeGroup({
      id: "gemini",
      levels: [Level.Warning]
    }),
    makeGroup({
      id: "opencode",
      levels: []
    })
  ];

  it("shows the first two agents as pills and folds the rest", () => {
    expect(triggerPillGroups(groups).map(group => group.id)).toEqual(["claude", "codex"]);
    expect(overflowCount(groups)).toBe(2);
    expect(overflowCount(groups.slice(0, 1))).toBe(0);
  });

  it("knows whether Claude keeps a pill of its own", () => {
    expect(hasClaudePill(groups)).toBe(true);
    expect(hasClaudePill(groups.slice(1))).toBe(false);
  });

  it("paints an agent with its worst limit's severity", () => {
    expect(
      groupSeverityClass(
        makeGroup({
          id: "codex",
          levels: [Level.Critical]
        })
      )
    ).toBe("critical");
    expect(
      groupSeverityClass(
        makeGroup({
          id: "opencode",
          levels: []
        })
      )
    ).toBeUndefined();
  });

  it("dots the overflow with the worst level any agent reaches", () => {
    expect(overflowLevel(groups)).toBe(Level.Warning);
    expect(
      overflowLevel([
        ...groups,
        makeGroup({
          id: "copilot",
          levels: [Level.Critical]
        })
      ])
    ).toBe(Level.Critical);
    expect(overflowLevel(groups.slice(0, 2))).toBeNull();
  });
});

describe("measuredAgentCount", () => {
  it("sums the agents that carry a severity", () => {
    expect(
      measuredAgentCount([
        {
          level: Level.Normal,
          label: "ok",
          count: 2
        },
        {
          level: Level.Critical,
          label: "critical",
          count: 1
        }
      ])
    ).toBe(3);
  });
});

describe("usageAriaLabel", () => {
  it("names the panel when no agent runs", () => {
    expect(usageAriaLabel([])).toBe(USAGE_PANEL_LABEL);
  });

  it("reads every agent's limits", () => {
    expect(
      usageAriaLabel([
        makeGroup({
          id: "claude",
          levels: [Level.Normal]
        })
      ])
    ).toBe("Usage - claude limit 0 50%");
  });
});

describe("isJustReset", () => {
  it("matches a limit by its agent and label", () => {
    const limit = makeLimit({
      label: "Session",
      level: Level.Normal
    });
    const justReset = new Set([
      limitKey({
        agentId: "claude",
        limit
      })
    ]);
    expect(
      isJustReset({
        justReset,
        agentId: "claude",
        limit
      })
    ).toBe(true);
    expect(
      isJustReset({
        justReset,
        agentId: "codex",
        limit
      })
    ).toBe(false);
  });
});

describe("pendingResetBoundaries", () => {
  const now = Date.parse("2026-10-15T12:00:00Z");
  const past = "2026-10-15T11:00:00Z";
  const soon = "2026-10-15T13:00:00Z";
  const later = "2026-10-16T12:00:00Z";
  const accounts = new Map([
    ["claude", makeAccount([later, past, null])],
    ["codex", makeAccount([soon])],
    ["gemini", null]
  ]);

  it("finds a passed boundary not yet refreshed, and the soonest ahead", () => {
    const { expired, next } = pendingResetBoundaries({
      accounts,
      refreshed: new Set(),
      now
    });
    expect(expired?.value).toBe(past);
    expect(next?.value).toBe(soon);
  });

  it("skips a passed boundary already refreshed for", () => {
    expect(
      pendingResetBoundaries({
        accounts,
        refreshed: new Set([past]),
        now
      }).expired
    ).toBeUndefined();
  });

  it("has nothing pending without reset times", () => {
    expect(
      pendingResetBoundaries({
        accounts: new Map(),
        refreshed: new Set(),
        now
      })
    ).toEqual({
      expired: undefined,
      next: undefined
    });
  });
});

describe("withLastGoodReadings", () => {
  const reading = {
    windows: [],
    plan: "Max",
    source: "test"
  };
  const NOW = 1_000_000_000;

  it("shows a fresh reading and keeps it", () => {
    const { accounts, kept } = withLastGoodReadings({
      fresh: new Map([["claude", reading]]),
      kept: new Map(),
      now: NOW
    });
    expect(accounts.get("claude")).toBe(reading);
    expect(kept.get("claude")?.at).toBe(NOW);
  });

  it("rides out a refused reading on the last real one", () => {
    const { accounts } = withLastGoodReadings({
      fresh: new Map([["claude", null]]),
      kept: new Map([["claude", {
        account: reading,
        at: NOW - KEEP_LAST_READING_MS + 1
      }]]),
      now: NOW
    });
    expect(accounts.get("claude")).toBe(reading);
  });

  it("lets a reading older than the limit go, and has nothing for an agent never read", () => {
    const { accounts } = withLastGoodReadings({
      fresh: new Map([["claude", null], ["codex", null]]),
      kept: new Map([["claude", {
        account: reading,
        at: NOW - KEEP_LAST_READING_MS - 1
      }]]),
      now: NOW
    });
    expect(accounts.get("claude")).toBeNull();
    expect(accounts.get("codex")).toBeNull();
  });
});
