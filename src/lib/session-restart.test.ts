import { mcpRestartTargets, rekeyLayout, schemeRestartTargets } from "@/lib/session-restart";
import type { AgentSession, McpChange } from "@/lib/types";
import { describe, expect, it } from "vitest";

const claude = {
  id: "claude",
  label: "Claude Code",
  command: "claude"
};
const codex = {
  id: "codex",
  label: "Codex",
  command: "codex"
};

function session(overrides: Partial<AgentSession> & { id: string }): AgentSession {
  return {
    agent: claude,
    conversationId: `conv-${overrides.id}`,
    ...overrides
  };
}

const change: McpChange = {
  path: "C:\\repositories\\avi\\poll-avi\\.mcp.json",
  agents: ["claude"],
  added: ["github"],
  removed: []
};
const project = "C:\\repositories\\avi\\poll-avi";

describe("mcpRestartTargets", () => {
  it("selects sessions of the governed agent in the changed directory", () => {
    const sessions = [session({ id: "a" }), session({ id: "b" })];
    const targets = mcpRestartTargets({
      sessions,
      change,
      currentProject: project
    });
    expect(targets.map(s => s.id)).toEqual(["a", "b"]);
  });

  it("skips agents the config doesn't govern", () => {
    const sessions = [session({
      id: "a",
      agent: codex
    }), session({ id: "b" })];
    const targets = mcpRestartTargets({
      sessions,
      change,
      currentProject: project
    });
    expect(targets.map(s => s.id)).toEqual(["b"]);
  });

  it("skips a worktree session whose own dir isn't the one that changed", () => {
    const worktree = session({
      id: "w",
      cwd: "C:\\repositories\\avi\\poll-avi-feature"
    });
    const targets = mcpRestartTargets({
      sessions: [worktree],
      change,
      currentProject: project
    });
    expect(targets).toEqual([]);
  });

  it("matches a session whose cwd IS the changed dir, up to path spelling", () => {
    const s = session({
      id: "s",
      cwd: "C:/repositories/avi/poll-avi"
    });
    const targets = mcpRestartTargets({
      sessions: [s],
      change,
      currentProject: project
    });
    expect(targets.map(t => t.id)).toEqual(["s"]);
  });

  it("skips a session with no conversation id to resume", () => {
    const s = session({
      id: "s",
      conversationId: undefined
    });
    const targets = mcpRestartTargets({
      sessions: [s],
      change,
      currentProject: project
    });
    expect(targets).toEqual([]);
  });
});

describe("schemeRestartTargets", () => {
  const fixedClaude = {
    ...claude,
    themeFixedAtSpawn: true
  };

  it("selects fixed-at-spawn sessions with a conversation to resume", () => {
    const sessions = [
      session({
        id: "a",
        agent: fixedClaude
      }),
      session({
        id: "b",
        agent: fixedClaude
      })
    ];
    expect(schemeRestartTargets({ sessions }).map(s => s.id)).toEqual(["a", "b"]);
  });

  it("skips an agent that re-themes live", () => {
    const sessions = [session({
      id: "shell",
      agent: {
        ...claude,
        themeFixedAtSpawn: false
      }
    })];
    expect(schemeRestartTargets({ sessions })).toEqual([]);
  });

  it("skips an agent with no themeFixedAtSpawn flag at all", () => {
    expect(schemeRestartTargets({ sessions: [session({ id: "a" })] })).toEqual([]);
  });

  it("skips a session with no conversation id to resume", () => {
    const sessions = [session({
      id: "a",
      agent: fixedClaude,
      conversationId: undefined
    })];
    expect(schemeRestartTargets({ sessions })).toEqual([]);
  });
});

describe("rekeyLayout", () => {
  it("re-keys the restarted sessions and drops their initial prompt", () => {
    const sessions = [
      session({
        id: "a",
        initialPrompt: "build it"
      }),
      session({ id: "b" })
    ];
    const relaid = rekeyLayout({
      sessions,
      paneIds: ["a", "b"],
      activeId: "a",
      rekeyed: {
        a: "a2"
      }
    });
    expect(relaid.sessions.map(s => s.id)).toEqual(["a2", "b"]);
    expect(relaid.sessions[0].initialPrompt).toBeUndefined();
    expect(relaid.sessions[0].conversationId).toBe("conv-a");
    expect(relaid.paneIds).toEqual(["a2", "b"]);
    expect(relaid.activeId).toBe("a2");
  });

  it("leaves untouched sessions and a non-restarted active id alone", () => {
    const relaid = rekeyLayout({
      sessions: [session({ id: "a" }), session({ id: "b" })],
      paneIds: ["a", "b"],
      activeId: "b",
      rekeyed: {
        a: "a2"
      }
    });
    expect(relaid.sessions.map(s => s.id)).toEqual(["a2", "b"]);
    expect(relaid.activeId).toBe("b");
  });

  it("keeps a null active id null", () => {
    const relaid = rekeyLayout({
      sessions: [session({ id: "a" })],
      paneIds: ["a"],
      activeId: null,
      rekeyed: {
        a: "a2"
      }
    });
    expect(relaid.activeId).toBeNull();
  });
});
