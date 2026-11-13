// Characterizes this window's session lifecycle - launch, select, close, exit,
// restart, hand-off and re-attach - against the public `createWindowSessions` API,
// with every store and the backend stubbed. Pins behaviour so the module can be
// split safely.

import { beforeEach, describe, expect, it, vi } from "vitest";

import { Command, sendMessage } from "@/lib/ipc/messaging";
import { forgetSession, rekeySession } from "@/lib/session/lifecycle/session-scope";
import { createWindowSessions } from "@/lib/shell/windowSessions.svelte";
import type { Agent, AgentSession } from "@/lib/types/agent";

const PROJECT = "C:\\work\\one";
const OTHER_PROJECT = "C:\\work\\two";
const FAILED_START_WINDOW_MS = 1;

vi.mock("@/lib/ipc/messaging", async importOriginal => {
  const messaging = await importOriginal<typeof import("@/lib/ipc/messaging")>();
  return {
    ...messaging,
    sendMessage: vi.fn(async () => undefined)
  };
});
vi.mock("@/lib/session/lifecycle/session-scope", () => ({
  forgetSession: vi.fn(),
  rekeySession: vi.fn()
}));
vi.mock("@/lib/shell/session-launch", () => ({
  newSession: ({
    options,
    project
  }: {
    options: {
      agent: Agent;
      sessionId?: string;
    };
    project: string;
  }) => ({
    id: options.sessionId ?? crypto.randomUUID(),
    agent: options.agent,
    project,
    args: []
  })
}));
vi.mock("@/lib/shell/windowSessionNaming.svelte", () => ({
  createSessionNaming: () => ({
    namesWithAi: () => false,
    seedNaming: vi.fn()
  })
}));
vi.mock("@/lib/stores/agent/agentRetheme.svelte", () => ({
  createAgentRetheme: () => ({
    recordSpawnSchemes: vi.fn(),
    followSchemeChange: vi.fn()
  })
}));
vi.mock("@/lib/stores/session/pendingPrompts.svelte", () => ({ setPendingPrompt: vi.fn() }));
vi.mock("@/lib/stores/session/sessionNaming.svelte", () => ({ setNaming: vi.fn() }));
vi.mock("@/lib/stores/session/sessions.svelte", () => ({
  isSessionIdle: () => true,
  whenSessionIdle: vi.fn(async () => undefined)
}));
vi.mock("@/lib/stores/ui/toast.svelte", () => ({ showToast: vi.fn() }));

const AGENT: Agent = {
  id: "claude",
  label: "Claude",
  command: "claude",
  reordersBidi: false
};

function setUp() {
  const host = {
    animateIn: vi.fn(),
    onLaunched: vi.fn(),
    onRetired: vi.fn(),
    onEmptied: vi.fn()
  };
  return {
    host,
    sessions: createWindowSessions(host)
  };
}

function launchTwo(windowSessions: ReturnType<typeof createWindowSessions>) {
  const first = windowSessions.launch({
    agent: AGENT,
    project: PROJECT
  });
  const second = windowSessions.launch({
    agent: AGENT,
    project: PROJECT,
    split: true
  });
  return {
    first,
    second
  };
}

beforeEach(() => {
  vi.mocked(sendMessage).mockClear();
  vi.mocked(forgetSession).mockClear();
  vi.mocked(rekeySession).mockClear();
});

describe("launch and select", () => {
  it("launches a single pane and tells the shell", () => {
    const { sessions, host } = setUp();
    const id = sessions.launch({
      agent: AGENT,
      project: PROJECT
    });

    expect(sessions.sessions.map(session => session.id)).toEqual([id]);
    expect(sessions.paneIds).toEqual([id]);
    expect(sessions.activeId).toBe(id);
    expect(sessions.activeProject).toBe(PROJECT);
    expect(host.onLaunched).toHaveBeenCalledTimes(1);
    expect(host.animateIn).not.toHaveBeenCalled();
  });

  it("adds a split pane beside the others and springs it in", () => {
    const { sessions, host } = setUp();
    const { first, second } = launchTwo(sessions);

    expect(sessions.paneIds).toEqual([first, second]);
    expect(sessions.activeId).toBe(second);
    expect(host.animateIn).toHaveBeenCalledWith(second);
  });

  it("a successor takes its predecessor's tab slot, pane and focus without announcing a launch", () => {
    const { sessions, host } = setUp();
    const { first, second } = launchTwo(sessions);
    host.onLaunched.mockClear();
    const successor = sessions.launch({
      agent: AGENT,
      project: PROJECT,
      replacing: first
    });

    expect(sessions.sessions.map(session => session.id)).toEqual([first, successor, second]);
    expect(sessions.paneIds).toEqual([successor, second]);
    expect(sessions.activeId).toBe(second);
    expect(host.onLaunched).not.toHaveBeenCalled();
  });

  it("select shows one session as the sole pane", () => {
    const { sessions } = setUp();
    const { first } = launchTwo(sessions);
    sessions.select(first);

    expect(sessions.paneIds).toEqual([first]);
    expect(sessions.activeId).toBe(first);
  });

  it("step wraps around the front project's tabs", () => {
    const { sessions } = setUp();
    const { first, second } = launchTwo(sessions);
    sessions.step(1);
    expect(sessions.activeId).toBe(first);
    sessions.step(-1);
    expect(sessions.activeId).toBe(second);
  });

  it("projectSessions follows the active session's project", () => {
    const { sessions } = setUp();
    const { first } = launchTwo(sessions);
    const elsewhere = sessions.launch({
      agent: AGENT,
      project: OTHER_PROJECT
    });

    expect(sessions.projectSessions.map(session => session.id)).toEqual([elsewhere]);
    sessions.select(first);
    expect(sessions.projectSessions).toHaveLength(2);
  });

  it("enter points the window at a project while it has no sessions", () => {
    const { sessions } = setUp();
    sessions.enter(OTHER_PROJECT);

    expect(sessions.activeProject).toBe(OTHER_PROJECT);
  });
});

describe("close", () => {
  it("kills the pty, detaches, reports retirement and keeps the project in front", async () => {
    const { sessions, host } = setUp();
    const id = sessions.launch({
      agent: AGENT,
      project: PROJECT
    });
    const [session] = sessions.sessions;
    await sessions.close(session);

    expect(sendMessage).toHaveBeenCalledWith(Command.ptyKill, { id });
    expect(sessions.sessions).toEqual([]);
    expect(forgetSession).toHaveBeenCalledWith(id);
    expect(host.onRetired).toHaveBeenCalledWith(session);
    expect(host.onEmptied).toHaveBeenCalledWith({
      agent: AGENT,
      project: PROJECT,
      isFailedStart: false
    });
    expect(sessions.activeProject).toBe(PROJECT);
  });

  it("does not empty the window while another session remains", async () => {
    const { sessions, host } = setUp();
    const { first } = launchTwo(sessions);
    await sessions.close(sessions.sessions[0]);

    expect(sessions.sessions.map(session => session.id)).not.toContain(first);
    expect(host.onEmptied).not.toHaveBeenCalled();
  });

  it("a stale close of an already-detached session does nothing more", async () => {
    const { sessions, host } = setUp();
    sessions.launch({
      agent: AGENT,
      project: PROJECT
    });
    const [session] = sessions.sessions;
    vi.mocked(sendMessage).mockImplementationOnce(async () => {
      sessions.release([session.id]);
    });
    await sessions.close(session);

    expect(host.onRetired).not.toHaveBeenCalled();
    expect(host.onEmptied).not.toHaveBeenCalled();
  });

  it("collapses a split pane that detaches", async () => {
    const { sessions } = setUp();
    const { second } = launchTwo(sessions);
    await sessions.close(sessions.sessions[1]);

    expect(sessions.collapsingSplitPanes.has(second)).toBe(true);
  });

  it("closeSessions kills them all and detaches each", async () => {
    const { sessions } = setUp();
    const { first, second } = launchTwo(sessions);
    await sessions.closeSessions([first, second]);

    expect(sendMessage).toHaveBeenCalledWith(Command.ptyKill, { id: first });
    expect(sendMessage).toHaveBeenCalledWith(Command.ptyKill, { id: second });
    expect(sessions.sessions).toEqual([]);
  });

  it("closeAllGracefully ends every session", async () => {
    const { sessions } = setUp();
    launchTwo(sessions);
    await sessions.closeAllGracefully();

    expect(sessions.sessions).toEqual([]);
  });
});

describe("exited", () => {
  it("ignores an exit claimed by a hand-close", async () => {
    const { sessions, host } = setUp();
    const id = sessions.launch({
      agent: AGENT,
      project: PROJECT
    });
    sessions.markClosing([id]);
    await sessions.exited(id);

    expect(sessions.sessions).toHaveLength(1);
    expect(host.onRetired).not.toHaveBeenCalled();
  });

  it("ignores an unknown id", async () => {
    const { sessions, host } = setUp();
    await sessions.exited("nope");

    expect(host.onRetired).not.toHaveBeenCalled();
  });

  it("retires a session that quit on its own and reports a failed start for the last one", async () => {
    const { sessions, host } = setUp();
    const id = sessions.launch({
      agent: AGENT,
      project: PROJECT
    });
    const [session] = sessions.sessions;
    await sessions.exited(id);

    expect(sessions.sessions).toEqual([]);
    expect(host.onRetired).toHaveBeenCalledWith(session);
    expect(host.onEmptied).toHaveBeenCalledWith({
      agent: AGENT,
      project: PROJECT,
      isFailedStart: expect.any(Boolean)
    });
  });

  it("does not report an empty window while others remain", async () => {
    const { sessions, host } = setUp();
    const { first } = launchTwo(sessions);
    await sessions.exited(first);

    expect(host.onEmptied).not.toHaveBeenCalled();
    expect(host.onRetired).toHaveBeenCalledTimes(1);
  });
});

describe("restart", () => {
  it("re-keys the targets to fresh ids and follows the layout", async () => {
    const { sessions, host } = setUp();
    const { first, second } = launchTwo(sessions);
    const [firstSession] = sessions.sessions;
    await sessions.restart([firstSession]);

    const ids = sessions.sessions.map(session => session.id);
    expect(ids).toHaveLength(2);
    expect(ids).not.toContain(first);
    expect(ids).toContain(second);
    expect(sessions.paneIds).toContain(second);
    expect(sessions.paneIds).not.toContain(first);
    expect(rekeySession).toHaveBeenCalledWith({
      fromId: first,
      toId: ids[0]
    });
    expect(host.onRetired).toHaveBeenCalledWith(firstSession);
  });

  it("applyResumeArgs hands a session its resume args", () => {
    const { sessions } = setUp();
    const id = sessions.launch({
      agent: AGENT,
      project: PROJECT
    });
    sessions.applyResumeArgs({
      id,
      args: ["--resume", "x"]
    });

    expect(sessions.sessions[0].args).toEqual(["--resume", "x"]);
    sessions.applyResumeArgs({
      id,
      args: []
    });
    expect(sessions.sessions[0].args).toEqual(["--resume", "x"]);
  });
});

describe("hand-off and re-attach", () => {
  function restored(id: string): AgentSession {
    return {
      id,
      agent: AGENT,
      project: PROJECT,
      args: []
    };
  }

  it("restore takes the layout and isOpen sees it", () => {
    const { sessions } = setUp();
    sessions.restore({
      sessions: [restored("r1")],
      paneIds: ["r1"],
      activeId: "r1"
    });

    expect(sessions.isOpen("r1")).toBe(true);
    expect(sessions.activeId).toBe("r1");
  });

  it("restore stamps a launch time, so a quick exit reads as a failed start", async () => {
    const { sessions, host } = setUp();
    sessions.restore({
      sessions: [restored("r1")],
      paneIds: ["r1"],
      activeId: "r1"
    });
    await sessions.exited("r1");

    expect(host.onEmptied).toHaveBeenCalledWith(expect.objectContaining({ isFailedStart: true }));
    expect(FAILED_START_WINDOW_MS).toBeGreaterThan(0);
  });

  it("removeClosed drops the sessions and forgets each", () => {
    const { sessions } = setUp();
    const { first, second } = launchTwo(sessions);
    sessions.removeClosed(new Set([first]));

    expect(sessions.sessions.map(session => session.id)).toEqual([second]);
    expect(forgetSession).toHaveBeenCalledWith(first);
  });

  it("release detaches without a kill or a retirement", () => {
    const { sessions, host } = setUp();
    const { first } = launchTwo(sessions);
    sessions.release([first]);

    expect(sessions.isOpen(first)).toBe(false);
    expect(sendMessage).not.toHaveBeenCalled();
    expect(host.onRetired).not.toHaveBeenCalled();
  });

  it("adopt takes in newcomers once, fronts the named one and announces a launch", () => {
    const { sessions, host } = setUp();
    sessions.adopt({
      adopted: [restored("a1"), restored("a2")],
      activeId: "a1"
    });
    sessions.adopt({
      adopted: [restored("a1")],
      activeId: null
    });

    expect(sessions.sessions.map(session => session.id)).toEqual(["a1", "a2"]);
    expect(sessions.activeId).toBe("a1");
    expect(sessions.paneIds).toEqual(["a1"]);
    expect(host.onLaunched).toHaveBeenCalledTimes(1);
  });

  it("removeHandedOff drops one session, leaves the active pane and reports retirement", () => {
    const { sessions, host } = setUp();
    const { first, second } = launchTwo(sessions);
    sessions.removeHandedOff(first);

    expect(sessions.sessions.map(session => session.id)).toEqual([second]);
    expect(sessions.activeId).toBe(second);
    expect(host.onRetired).toHaveBeenCalledTimes(1);
    expect(forgetSession).toHaveBeenCalledWith(first);
  });
});
