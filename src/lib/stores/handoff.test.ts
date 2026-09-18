import { dropAgentActivity, observeAgentActivity } from "@/lib/stores/agentActivity.svelte";
import {
  createAutoHandoff,
  handoffDocName,
  handoffPrompt,
  HandoffReason,
  handoffRequestBody,
  handoffSlug,
  hasUsageHeadroom,
  pickHandoffSuccessor,
  pickSuccessor,
  successorPrompt
} from "@/lib/stores/handoff.svelte";
import { BRACKETED_PASTE_END, PROMPT_SUBMIT } from "@/lib/terminal-input";
import { CreditsState } from "@/lib/types";
import type { Agent, AgentSession } from "@/lib/types";
import {
  afterEach,
  beforeEach,
  describe,
  expect,
  it,
  vi
} from "vitest";

const bridgeMocks = vi.hoisted(() => ({
  kill: vi.fn<(id: string) => Promise<void>>(),
  write: vi.fn<(options: {
    id: string;
    data: string;
  }) => Promise<void>>(),
  probePath: vi.fn<(path: string) => Promise<{ isFile: boolean }>>(),
  deleteHandoffDoc: vi.fn<() => Promise<void>>()
}));

vi.mock("@/lib/bridge", () => ({
  feed: {
    onChange: vi.fn()
  },
  pty: {
    kill: bridgeMocks.kill,
    write: bridgeMocks.write
  },
  usage: {
    get: vi.fn().mockResolvedValue(null),
    accountFor: vi.fn().mockResolvedValue(null)
  },
  workspace: {
    probePath: bridgeMocks.probePath,
    deleteHandoffDoc: bridgeMocks.deleteHandoffDoc
  }
}));

vi.mock("@/lib/stores/context.svelte", () => ({
  dropContext: vi.fn(),
  measuredContextPercentage: vi.fn().mockReturnValue(100)
}));

vi.mock("@/lib/stores/sessions.svelte", () => ({
  dropSessionStatus: vi.fn(),
  sessionStatus: vi.fn().mockReturnValue("ready")
}));

beforeEach(() => {
  vi.clearAllMocks();
  bridgeMocks.kill.mockResolvedValue();
  bridgeMocks.probePath.mockResolvedValue({ isFile: true });
  bridgeMocks.deleteHandoffDoc.mockResolvedValue();

  const storage = new Map<string, string>();
  vi.stubGlobal("sessionStorage", {
    getItem: (key: string) => storage.get(key) ?? null,
    setItem: (key: string, value: string) => storage.set(key, value),
    removeItem: (key: string) => storage.delete(key)
  });
});

afterEach(() => vi.unstubAllGlobals());

describe("handoffSlug", () => {
  it("lowercases and kebabs a workspace label", () => {
    expect(handoffSlug("My Project!")).toBe("my-project");
  });

  it("flattens path separators from a short dir", () => {
    expect(handoffSlug("avi/pade")).toBe("avi-pade");
  });

  it("keeps an already-safe label unchanged", () => {
    expect(handoffSlug("temp-42")).toBe("temp-42");
  });

  it("collapses punctuation runs and strips the edges", () => {
    expect(handoffSlug("--wip: retry loop--")).toBe("wip-retry-loop");
  });

  it("falls back to a generic slug when nothing survives", () => {
    expect(handoffSlug("!!!")).toBe("session");
    expect(handoffSlug("")).toBe("session");
  });
});

describe("handoffDocName", () => {
  it("includes the session token so same-project sessions never collide", () => {
    const source = "pade";
    const first = handoffDocName({
      source,
      sessionId: "1a2b3c4d-1111-2222-3333-444455556666"
    });
    const second = handoffDocName({
      source,
      sessionId: "9f8e7d6c-1111-2222-3333-444455556666"
    });
    expect(first).toBe("continue-pade-1a2b3c4d.md");
    expect(second).toBe("continue-pade-9f8e7d6c.md");
    expect(first).not.toBe(second);
  });

  it("falls back to a generic token for a non-UUID session id", () => {
    expect(
      handoffDocName({
        source: "pade",
        sessionId: ""
      })
    ).toBe("continue-pade-session.md");
  });
});

describe("handoff prompts", () => {
  const documentName = "continue-pade-1a2b3c4d.md";

  it("explains that an MCP handoff will reload project configuration", () => {
    const prompt = handoffPrompt({
      doc: documentName,
      reason: HandoffReason.ConfigurationChange
    });
    expect(prompt).toContain("MCP server configuration changed");
    expect(prompt).toContain(`handoff to ${documentName}`);
  });

  it("keeps the request body free of paste framing and the submit keystroke", () => {
    // The store pastes this body and submits it with a SEPARATE, retried Enter;
    // a body carrying its own CR or paste marker would defeat that delivery.
    const body = handoffRequestBody({
      doc: documentName,
      reason: HandoffReason.ContextLimit
    });
    expect(body).not.toContain(PROMPT_SUBMIT);
    expect(body).not.toContain(BRACKETED_PASTE_END);
    expect(body).toContain(`handoff to ${documentName}`);
  });

  it("explains a save handoff is for restarting in the saved location", () => {
    const body = handoffRequestBody({
      doc: documentName,
      reason: HandoffReason.Save
    });
    expect(body).toContain("saved as a permanent project");
    expect(body).toContain(`handoff to ${documentName}`);
  });

  it("seeds the successor to continue from the written document", () => {
    expect(successorPrompt(documentName)).toBe(
      `Read ${documentName} to continue the work where the previous session left off.`
    );
  });
});

describe("hasUsageHeadroom", () => {
  it("keeps an agent whose quota still has room", () => {
    expect(
      hasUsageHeadroom({
        usedPercentage: 93,
        credits: CreditsState.enum.off
      })
    ).toBe(true);
  });

  it("keeps an agent whose quota is nearly — but not fully — spent", () => {
    // What sent a handoff to another agent while Claude could still work: its
    // weekly cap read 96%, which is quota left, not quota gone.
    expect(
      hasUsageHeadroom({
        usedPercentage: 96,
        credits: CreditsState.enum.off
      })
    ).toBe(true);
  });

  it("counts a spent quota as headroom while usage credits carry it", () => {
    expect(
      hasUsageHeadroom({
        usedPercentage: 100,
        credits: CreditsState.enum.available
      })
    ).toBe(true);
  });

  it("has none when the quota is spent and credits cannot carry it", () => {
    expect(
      hasUsageHeadroom({
        usedPercentage: 100,
        credits: CreditsState.enum.unavailable
      })
    ).toBe(false);
    expect(
      hasUsageHeadroom({
        usedPercentage: 100,
        credits: null
      })
    ).toBe(false);
  });

  it("treats an unknown quota as enough", () => {
    expect(
      hasUsageHeadroom({
        usedPercentage: null,
        credits: null
      })
    ).toBe(true);
  });
});

describe("pickSuccessor", () => {
  function agent(id: string): Agent {
    return {
      id,
      label: id,
      command: id,
      reordersBidi: false
    };
  }

  const claude = agent("claude");
  const codex = agent("codex");
  const gemini = agent("gemini");
  const available = [claude, codex, gemini];
  function headroomFor(ids: string[]) {
    return (agentId: string) => Promise.resolve(ids.includes(agentId));
  }

  it("keeps the current agent while it still has headroom", async () => {
    const chosen = await pickSuccessor({
      current: claude,
      available,
      hasHeadroom: headroomFor(["claude", "codex"])
    });
    expect(chosen).toBe(claude);
  });

  it("crosses over to the first other agent with headroom", async () => {
    const chosen = await pickSuccessor({
      current: claude,
      available,
      hasHeadroom: headroomFor(["codex", "gemini"])
    });
    expect(chosen).toBe(codex);
  });

  it("skips agents without headroom to find one that has it", async () => {
    const chosen = await pickSuccessor({
      current: claude,
      available,
      hasHeadroom: headroomFor(["gemini"])
    });
    expect(chosen).toBe(gemini);
  });

  it("returns null when no agent has headroom", async () => {
    const chosen = await pickSuccessor({
      current: codex,
      available,
      hasHeadroom: headroomFor([])
    });
    expect(chosen).toBeNull();
  });
});

describe("pickHandoffSuccessor", () => {
  const claude: Agent = {
    id: "claude",
    label: "Claude Code",
    command: "claude",
    reordersBidi: true
  };
  const codex: Agent = {
    id: "codex",
    label: "Codex",
    command: "codex",
    reordersBidi: false
  };

  it("pins a configuration handoff to the governed agent", async () => {
    const successor = await pickHandoffSuccessor({
      reason: HandoffReason.ConfigurationChange,
      current: claude,
      available: [claude, codex],
      hasHeadroom: agentId => Promise.resolve(agentId === codex.id)
    });
    expect(successor).toBe(claude);
  });
});

describe("createAutoHandoff", () => {
  it("terminates and removes the predecessor before launching its successor", async () => {
    const predecessor: AgentSession = {
      id: "old-session",
      agent: {
        id: "claude",
        label: "Claude Code",
        command: "claude",
        reordersBidi: true
      },
      cwd: "C:/repositories/pade"
    };
    let sessions = [predecessor];
    const lifecycle: string[] = [];
    function killNotYetAwaited(): void {}
    let finishKill: () => void = killNotYetAwaited;
    bridgeMocks.kill.mockImplementation(async () => {
      lifecycle.push("kill-started");
      await new Promise<void>(resolve => {
        finishKill = resolve;
      });
      lifecycle.push("kill-finished");
    });

    const handoff = createAutoHandoff({
      sessions: () => sessions,
      availableAgents: () => [predecessor.agent],
      isOptedOut: () => false,
      thresholdPercentage: () => 90,
      slugSource: () => "pade",
      projectDirectory: () => "C:/repositories/pade",
      // The shell's announced-close path: it flags the kill so its own exit
      // event isn't read as the agent quitting (which would respawn a bare tab).
      async endSession(id) {
        await bridgeMocks.kill(id);
      },
      removeSession(id) {
        lifecycle.push("predecessor-removed");
        sessions = sessions.filter(session => session.id !== id);
      },
      launchSuccessor() {
        lifecycle.push("successor-launched");
        expect(sessions).not.toContainEqual(predecessor);
        return "new-session";
      }
    });

    handoff.force(predecessor);
    await vi.waitFor(() => expect(bridgeMocks.kill).toHaveBeenCalledWith(predecessor.id));
    expect(lifecycle).toEqual(["kill-started"]);

    finishKill();
    await vi.waitFor(() => expect(lifecycle).toContain("successor-launched"));

    expect(lifecycle).toEqual([
      "kill-started",
      "kill-finished",
      "predecessor-removed",
      "successor-launched"
    ]);
    handoff.dispose();
  });

  it("ends the predecessor through the shell so its exit isn't read as the agent quitting", async () => {
    const predecessor: AgentSession = {
      id: "old-session",
      agent: {
        id: "claude",
        label: "Claude Code",
        command: "claude",
        reordersBidi: true
      },
      cwd: "C:/repositories/pade"
    };
    let sessions = [predecessor];

    // The shell respawns a replacement when the LAST session exits unannounced.
    // Killing straight through the bridge left that flag unset, so the handoff's
    // own kill raced its exit event into a bare extra tab that also stole the
    // active pane from the successor. Model it: an exit is only "announced" if
    // the handoff went through endSession first.
    let announced = false;
    let spuriousRespawns = 0;
    bridgeMocks.kill.mockImplementation(async (id: string) => {
      // The exit event lands while the kill is still settling — before the
      // shell has had a chance to drop the session.
      const stillListed = sessions.some(session => session.id === id);
      if (!announced && stillListed) {
        spuriousRespawns += 1;
      }
    });

    const handoff = createAutoHandoff({
      sessions: () => sessions,
      availableAgents: () => [predecessor.agent],
      isOptedOut: () => false,
      thresholdPercentage: () => 90,
      slugSource: () => "pade",
      projectDirectory: () => "C:/repositories/pade",
      async endSession(id) {
        announced = true;
        await bridgeMocks.kill(id);
      },
      removeSession(id) {
        sessions = sessions.filter(session => session.id !== id);
        announced = false;
      },
      launchSuccessor: () => "new-session"
    });

    handoff.force(predecessor);
    await vi.waitFor(() => expect(sessions).toHaveLength(0));

    expect(spuriousRespawns).toBe(0);
    handoff.dispose();
  });

  it("holds a handoff until the agent's background work finishes", async () => {
    const ESCAPE = "\u001b";
    const BELL = "\u0007";
    const predecessor: AgentSession = {
      id: "workflow-session",
      agent: {
        id: "claude",
        label: "Claude Code",
        command: "claude",
        reordersBidi: true
      },
      cwd: "C:/repositories/pade"
    };
    let sessions = [predecessor];
    const launched: string[] = [];
    // A dynamic workflow is running: the session is quiet, but Claude Code's title
    // still carries its busy glyph.
    observeAgentActivity({
      id: predecessor.id,
      chunk: `${ESCAPE}]0;◐ Finish the curriculum repair${BELL}`
    });

    const handoff = createAutoHandoff({
      sessions: () => sessions,
      availableAgents: () => [predecessor.agent],
      isOptedOut: () => false,
      thresholdPercentage: () => 90,
      slugSource: () => "pade",
      projectDirectory: () => "C:/repositories/pade",
      async endSession(id) {
        await bridgeMocks.kill(id);
      },
      removeSession(id) {
        sessions = sessions.filter(session => session.id !== id);
      },
      launchSuccessor() {
        launched.push("successor");
        return "successor-session";
      }
    });

    handoff.force(predecessor);
    await vi.waitFor(() => expect(handoff.note).toContain("once its running work finishes"));
    expect(bridgeMocks.write).not.toHaveBeenCalled();
    expect(bridgeMocks.kill).not.toHaveBeenCalled();

    observeAgentActivity({
      id: predecessor.id,
      chunk: `${ESCAPE}]0;✳ Finish the curriculum repair${BELL}`
    });
    await vi.waitFor(() => expect(launched).toEqual(["successor"]));
    expect(bridgeMocks.kill).toHaveBeenCalledWith(predecessor.id);

    handoff.dispose();
    dropAgentActivity(predecessor.id);
  });

  it("holds a near-limit session back in the scan while its agent reports work", async () => {
    const ESCAPE = "\u001b";
    const BELL = "\u0007";
    const session: AgentSession = {
      id: "scanned-session",
      agent: {
        id: "claude",
        label: "Claude Code",
        command: "claude",
        reordersBidi: true
      },
      cwd: "C:/repositories/pade"
    };
    observeAgentActivity({
      id: session.id,
      chunk: `${ESCAPE}]0;◑ Waiting on a workflow${BELL}`
    });

    const launched: string[] = [];
    const handoff = createAutoHandoff({
      sessions: () => [session],
      availableAgents: () => [session.agent],
      isOptedOut: () => false,
      thresholdPercentage: () => 90,
      slugSource: () => "pade",
      projectDirectory: () => "C:/repositories/pade",
      endSession: bridgeMocks.kill,
      removeSession() {},
      launchSuccessor() {
        launched.push("successor");
        return "successor-session";
      }
    });

    handoff.check();
    expect(handoff.note).toBe("Claude Code context at 100% — handing off once its running work finishes…");
    await Promise.resolve();
    expect(bridgeMocks.kill).not.toHaveBeenCalled();
    expect(launched).toEqual([]);

    observeAgentActivity({
      id: session.id,
      chunk: `${ESCAPE}]0;✳ Waiting on a workflow${BELL}`
    });
    handoff.check();
    await vi.waitFor(() => expect(launched).toEqual(["successor"]));
    expect(bridgeMocks.kill).toHaveBeenCalledWith(session.id);

    handoff.dispose();
    dropAgentActivity(session.id);
  });
});
