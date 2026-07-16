import { SHELL_AGENT_ID } from "@/lib/types";
import type { AccountUsage, AgentSession } from "@/lib/types";
import { buildGroups, buildKindLegend, findSpotlight, severityBreakdown } from "@/lib/usageGroups";
import type { Level, SeveritySlice } from "@/lib/usageGroups";
import { describe, expect, it } from "vitest";

// A running session bound to `agentId`; only `session.agent` matters to the
// grouping, so the session id just needs to be distinct for the dedupe test.
function makeSession({ agentId, label, sessionId }: {
  agentId: string;
  label?: string;
  sessionId?: string;
}): AgentSession {
  return {
    id: sessionId ?? `session-${agentId}`,
    agent: {
      id: agentId,
      label: label ?? agentId,
      command: agentId
    }
  };
}

// A Claude account: 5-hour at `fiveHour`, weekly at `sevenDay`, plus any models.
function makeAccount(over: Partial<AccountUsage> = {}): AccountUsage {
  return {
    fiveHour: {
      utilization: 40
    },
    sevenDay: {
      utilization: 80
    },
    models: [],
    plan: "Max",
    source: "test",
    ...over
  };
}

const CLAUDE_LABEL = "Claude Code";

function claudeSession(sessionId?: string): AgentSession {
  return makeSession({
    agentId: "claude",
    label: CLAUDE_LABEL,
    sessionId
  });
}

function countAt({ slices, level }: {
  slices: SeveritySlice[];
  level: Level;
}): number {
  return slices.find(slice => slice.level === level)?.count ?? 0;
}

describe("buildGroups", () => {
  const now = Date.UTC(2026, 0, 1);

  it("dedupes agents by id, keeping the first occurrence", () => {
    const groups = buildGroups({
      account: makeAccount(),
      sessions: [claudeSession("first"), claudeSession("second")],
      now
    });

    expect(groups).toHaveLength(1);
    expect(groups[0].id).toBe("claude");
  });

  it("excludes the shell fallback and terminal-editor sessions", () => {
    const groups = buildGroups({
      account: makeAccount(),
      sessions: [
        makeSession({
          agentId: SHELL_AGENT_ID,
          label: "Terminal (shell)"
        }),
        makeSession({
          agentId: "editor-nvim",
          label: "Neovim"
        }),
        claudeSession()
      ],
      now
    });

    expect(groups.map(group => group.id)).toEqual(["claude"]);
  });

  it("populates the Claude group with real limits + plan from the account", () => {
    const [claude] = buildGroups({
      account: makeAccount(),
      sessions: [claudeSession()],
      now
    });

    expect(claude.unknown).toBe(false);
    expect(claude.plan).toBe("Max");
    expect(claude.icon).toBe("sparkles");
    expect(claude.name).toBe(CLAUDE_LABEL);
    expect(claude.shortName).toBe("Claude");
    expect(claude.limits.map(limit => limit.kindShort)).toEqual(["5h", "wk"]);
    expect(claude.limits.map(limit => limit.pct)).toEqual([40, 80]);
    expect(claude.limits.map(limit => limit.level)).toEqual(["normal", "warn"]);
  });

  it("marks every non-Claude agent unknown, with no limits or plan", () => {
    const [codex] = buildGroups({
      account: makeAccount(),
      sessions: [makeSession({
        agentId: "codex",
        label: "Codex"
      })],
      now
    });

    expect(codex.unknown).toBe(true);
    expect(codex.limits).toEqual([]);
    expect(codex.plan).toBe("");
    expect(codex.shortName).toBe("Codex");
  });

  it("keeps Claude present but empty when the account is null", () => {
    const [claude] = buildGroups({
      account: null,
      sessions: [claudeSession()],
      now
    });

    expect(claude.unknown).toBe(false);
    expect(claude.limits).toEqual([]);
    expect(claude.plan).toBe("");
  });

  it("sorts agents with limits ahead of unknown ones, worst-first", () => {
    const groups = buildGroups({
      account: makeAccount(),
      sessions: [
        makeSession({
          agentId: "codex",
          label: "Codex"
        }),
        claudeSession(),
        makeSession({
          agentId: "aider",
          label: "aider"
        })
      ],
      now
    });

    // Claude (the only agent with limits) leads; the two unknowns sink to the
    // end in their first-seen order (a stable sort).
    expect(groups.map(group => group.id)).toEqual(["claude", "codex", "aider"]);
  });

  it("counts every distinct agent — the few/many boundary sits at 2", () => {
    const twoAgents = buildGroups({
      account: makeAccount(),
      sessions: [claudeSession(), makeSession({ agentId: "codex" })],
      now
    });
    const threeAgents = buildGroups({
      account: makeAccount(),
      sessions: [
        claudeSession(),
        makeSession({ agentId: "codex" }),
        makeSession({ agentId: "aider" })
      ],
      now
    });

    // The trigger renders wide chips while the count is ≤ 2 and switches to
    // compact pills + "+N" once it exceeds 2.
    expect(twoAgents).toHaveLength(2);
    expect(threeAgents).toHaveLength(3);
  });
});

describe("panel view-model skips unknown agents", () => {
  const now = Date.UTC(2026, 0, 1);
  // Claude at a critical per-model cap, plus an unknown Codex agent alongside.
  const groups = buildGroups({
    account: makeAccount({
      sevenDay: {
        utilization: 88
      },
      models: [
        {
          name: "Claude Opus",
          utilization: 96
        }
      ]
    }),
    sessions: [claudeSession(), makeSession({
      agentId: "codex",
      label: "Codex"
    })],
    now
  });

  it("still counts the unknown agent in the running total", () => {
    expect(groups).toHaveLength(2);
    expect(groups.some(group => group.unknown)).toBe(true);
  });

  it("buckets only agents that have limits by severity", () => {
    const slices = severityBreakdown(groups);

    // Claude's worst limit is the 96% Opus cap → one critical, nothing else.
    // Codex has no limits, so it never lands in a severity bucket.
    expect(
      countAt({
        slices,
        level: "crit"
      })
    ).toBe(1);
    expect(
      countAt({
        slices,
        level: "warn"
      })
    ).toBe(0);
    expect(
      countAt({
        slices,
        level: "normal"
      })
    ).toBe(0);
  });

  it("spotlights the single closest limit, ignoring unknown agents", () => {
    const spotlight = findSpotlight(groups);

    expect(spotlight?.agent.id).toBe("claude");
    expect(spotlight?.limit.pct).toBe(96);
  });

  it("builds the kind legend only from agents with limits", () => {
    const legend = buildKindLegend(groups);

    expect(legend.map(entry => entry.short)).toEqual(["5h", "wk", "OP"]);
  });
});
