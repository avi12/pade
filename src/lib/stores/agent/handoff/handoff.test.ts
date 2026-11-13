import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import { Command } from "@/lib/ipc/messaging";
import { forgetSession } from "@/lib/session/lifecycle/session-scope";
import { observeAgentActivity } from "@/lib/stores/agent/agentActivity.svelte";
import { compactionHeadroom, measuredContextPercentage } from "@/lib/stores/agent/context.svelte";
import { successorPrompt } from "@/lib/stores/agent/handoff/handoff-prompts";
import { createAutoHandoff } from "@/lib/stores/agent/handoff/handoff.svelte";
import { sessionStatus } from "@/lib/stores/session/sessions.svelte";
import { SessionStatus } from "@/lib/types/agent";
import type { AgentSession } from "@/lib/types/agent";

const mocks = vi.hoisted(() => ({
  kill: vi.fn<(id: string) => Promise<void>>(),
  sendMessage: vi.fn<(command: Command, data?: unknown) => Promise<unknown>>(),
  onMessage: vi.fn()
}));

vi.mock("@/lib/ipc/messaging", async importOriginal => ({
  ...(await importOriginal<typeof import("@/lib/ipc/messaging")>()),
  sendMessage: mocks.sendMessage,
  onMessage: mocks.onMessage
}));

/** The backend as the handoff sees it: every probed file is on disk, usage
 *  reads come back empty, and writes / doc deletes simply succeed. */
async function answerMessage(command: Command) {
  if (command === Command.workspaceProbePath) {
    return { isFile: true };
  }

  if (command === Command.usageGet || command === Command.usageAccountAgent) {
    return null;
  }

  return undefined;
}

vi.mock("@/lib/stores/agent/context.svelte", () => ({
  compactionHeadroom: vi.fn().mockReturnValue(null),
  measuredContextPercentage: vi.fn().mockReturnValue(100)
}));

vi.mock("@/lib/stores/session/sessions.svelte", () => ({
  sessionStatus: vi.fn().mockReturnValue("ready")
}));

beforeEach(() => {
  vi.clearAllMocks();
  mocks.kill.mockResolvedValue();
  mocks.sendMessage.mockImplementation(answerMessage);

  const storage = new Map<string, string>();
  vi.stubGlobal("sessionStorage", {
    getItem: (key: string) => storage.get(key) ?? null,
    setItem: (key: string, value: string) => storage.set(key, value),
    removeItem: (key: string) => storage.delete(key)
  });
});

afterEach(() => vi.unstubAllGlobals());

describe("createAutoHandoff", () => {
  it("terminates the predecessor before launching its successor, and removes it right after", async () => {
    const predecessor: AgentSession = {
      id: "old-session",
      agent: {
        id: "claude",
        label: "Claude Code",
        command: "claude",
        reordersBidi: true
      },
      cwd: "C:/repositories/pade",
      project: "C:/repositories/pade"
    };
    let sessions = [predecessor];
    const lifecycle: string[] = [];
    function killNotYetAwaited() {}
    let finishKill: () => void = killNotYetAwaited;
    mocks.kill.mockImplementation(async () => {
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
        await mocks.kill(id);
      },
      removeSession(id) {
        lifecycle.push("predecessor-removed");
        sessions = sessions.filter(session => session.id !== id);
      },
      launchSuccessor() {
        lifecycle.push("successor-launched");
        // Still there, so the successor can take over what was kept for it.
        expect(sessions).toContainEqual(predecessor);
        return "new-session";
      }
    });

    handoff.force(predecessor);
    await vi.waitFor(() => expect(mocks.kill).toHaveBeenCalledWith(predecessor.id));
    expect(lifecycle).toEqual(["kill-started"]);

    finishKill();
    await vi.waitFor(() => expect(lifecycle).toContain("successor-launched"));

    expect(lifecycle).toEqual(["kill-started", "kill-finished", "successor-launched", "predecessor-removed"]);
    handoff.dispose();
  });

  it("names the doc by its full project path, whatever folder the agent works in", async () => {
    const predecessor: AgentSession = {
      id: "1a2b3c4d-session",
      agent: {
        id: "claude",
        label: "Claude Code",
        command: "claude",
        reordersBidi: true
      },
      cwd: "C:/repositories/pade/packages/app",
      project: "C:/repositories/pade"
    };
    const docPath = "C:/repositories/pade/continue-pade-1a2b3c4d.md";
    let isDocWritten = false;
    mocks.sendMessage.mockImplementation(async command => {
      if (command === Command.workspaceProbePath) {
        return { isFile: isDocWritten };
      }

      if (command === Command.ptyWrite) {
        // The agent takes the request and writes the doc.
        vi.mocked(sessionStatus).mockReturnValue(SessionStatus.enum.working);
        isDocWritten = true;
      }

      return await answerMessage(command);
    });
    function stopListening() {}
    mocks.onMessage.mockResolvedValue(stopListening);
    let successorSeed: string | undefined;
    const handoff = createAutoHandoff({
      sessions: () => [predecessor],
      availableAgents: () => [predecessor.agent],
      isOptedOut: () => false,
      thresholdPercentage: () => 90,
      slugSource: () => "pade",
      projectDirectory: session => session.project,
      endSession: mocks.kill,
      removeSession() {},
      launchSuccessor({ initialPrompt }) {
        successorSeed = initialPrompt;
        return "new-session";
      }
    });

    handoff.force(predecessor);
    // The doc lands after the request is typed, so the wait takes a disk poll.
    await vi.waitFor(() => expect(successorSeed).toBeDefined(), { timeout: 5_000 });

    const typed = mocks.sendMessage.mock.calls
      .filter(([command]) => command === Command.ptyWrite)
      .map(([, data]) => JSON.stringify(data))
      .join("");
    expect(typed).toContain(docPath);
    expect(successorSeed).toBe(successorPrompt(docPath));
    handoff.dispose();
    vi.mocked(sessionStatus).mockReturnValue(SessionStatus.enum.ready);
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
      cwd: "C:/repositories/pade",
      project: "C:/repositories/pade"
    };
    let sessions = [predecessor];

    // The shell respawns a replacement when the LAST session exits unannounced.
    // Killing straight through `pty_kill` left that flag unset, so the handoff's
    // own kill raced its exit event into a bare extra tab that also stole the
    // active pane from the successor. Model it: an exit is only "announced" if
    // the handoff went through endSession first.
    let announced = false;
    let spuriousRespawns = 0;
    mocks.kill.mockImplementation(async (id: string) => {
      // The exit event lands while the kill is still settling - before the
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
        await mocks.kill(id);
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
      cwd: "C:/repositories/pade",
      project: "C:/repositories/pade"
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
        await mocks.kill(id);
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
    expect(mocks.sendMessage).not.toHaveBeenCalledWith(Command.ptyWrite, expect.anything());
    expect(mocks.kill).not.toHaveBeenCalled();

    observeAgentActivity({
      id: predecessor.id,
      chunk: `${ESCAPE}]0;✳ Finish the curriculum repair${BELL}`
    });
    await vi.waitFor(() => expect(launched).toEqual(["successor"]));
    expect(mocks.kill).toHaveBeenCalledWith(predecessor.id);

    handoff.dispose();
    forgetSession(predecessor.id);
  });

  // The handoff doc is the whole session's work. Retiring it on anything short of
  // the successor's own first turn is how one got deleted unread: the successor
  // printed an update banner, rejected the model its account was configured for,
  // and its boot output alone looked like a finished turn.
  describe("retiring the handoff doc", () => {
    const ESCAPE = "\u001b";
    const BELL = "\u0007";
    const predecessor: AgentSession = {
      id: "old-session",
      agent: {
        id: "claude",
        label: "Claude Code",
        command: "claude",
        reordersBidi: true
      },
      cwd: "C:/repositories/pade",
      project: "C:/repositories/pade"
    };

    // The waits here are minutes long; fake timers run them in place.
    beforeEach(() => vi.useFakeTimers());
    afterEach(() => vi.useRealTimers());

    function handoffTo(successorId: string) {
      let sessions = [
        predecessor,
        {
          ...predecessor,
          id: successorId
        }
      ];
      return createAutoHandoff({
        sessions: () => sessions,
        availableAgents: () => [predecessor.agent],
        isOptedOut: () => false,
        thresholdPercentage: () => 90,
        slugSource: () => "pade",
        projectDirectory: () => "C:/repositories/pade",
        endSession: mocks.kill,
        removeSession(id) {
          sessions = sessions.filter(session => session.id !== id);
        },
        launchSuccessor: () => successorId
      });
    }

    it("deletes the doc once the successor's own turn has finished", async () => {
      const handoff = handoffTo("reading-successor");
      handoff.force(predecessor);
      await vi.advanceTimersByTimeAsync(20_000);
      expect(mocks.kill).toHaveBeenCalledWith(predecessor.id);

      observeAgentActivity({
        id: "reading-successor",
        chunk: `${ESCAPE}]0;◐ Reading the handoff${BELL}`
      });
      await vi.advanceTimersByTimeAsync(4_000);
      expect(mocks.sendMessage).not.toHaveBeenCalledWith(Command.handoffDocDelete, expect.anything());

      observeAgentActivity({
        id: "reading-successor",
        chunk: `${ESCAPE}]0;✳ Reading the handoff${BELL}`
      });
      await vi.advanceTimersByTimeAsync(4_000);
      expect(mocks.sendMessage).toHaveBeenCalledWith(
        Command.handoffDocDelete,
        expect.objectContaining({
          dir: "C:/repositories/pade"
        })
      );

      handoff.dispose();
      forgetSession("reading-successor");
    });

    it("keeps the doc when the successor never reports a turn", async () => {
      const handoff = handoffTo("broken-successor");
      handoff.force(predecessor);
      await vi.advanceTimersByTimeAsync(20_000);
      expect(mocks.kill).toHaveBeenCalledWith(predecessor.id);

      // Ten minutes of a successor that boots, errors and reports nothing.
      await vi.advanceTimersByTimeAsync(11 * 60_000);
      expect(mocks.sendMessage).not.toHaveBeenCalledWith(Command.handoffDocDelete, expect.anything());

      handoff.dispose();
    });
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
      cwd: "C:/repositories/pade",
      project: "C:/repositories/pade"
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
      endSession: mocks.kill,
      removeSession() {},
      launchSuccessor() {
        launched.push("successor");
        return "successor-session";
      }
    });

    handoff.check();
    expect(handoff.note).toBe(WAITING_NOTE);
    await Promise.resolve();
    expect(mocks.kill).not.toHaveBeenCalled();
    expect(launched).toEqual([]);

    observeAgentActivity({
      id: session.id,
      chunk: `${ESCAPE}]0;✳ Waiting on a workflow${BELL}`
    });
    handoff.check();
    await vi.waitFor(() => expect(launched).toEqual(["successor"]));
    expect(mocks.kill).toHaveBeenCalledWith(session.id);

    handoff.dispose();
    forgetSession(session.id);
  });

  it("says it is waiting while a near-limit session's turn runs", () => {
    vi.mocked(sessionStatus).mockReturnValue(SessionStatus.enum.working);
    const handoff = createAutoHandoff(scanHost(scannedSession));

    handoff.check();

    expect(handoff.note).toBe(WAITING_NOTE);
    expect(mocks.kill).not.toHaveBeenCalled();
    handoff.dispose();
    vi.mocked(sessionStatus).mockReturnValue(SessionStatus.enum.ready);
  });

  it("hands off mid-turn once the agent is about to auto-compact", async () => {
    const ESCAPE = "\u001b";
    const BELL = "\u0007";
    observeAgentActivity({
      id: scannedSession.id,
      chunk: `${ESCAPE}]0;◑ Working${BELL}`
    });
    vi.mocked(measuredContextPercentage).mockReturnValue(40);
    vi.mocked(compactionHeadroom).mockReturnValue(2);
    const handoff = createAutoHandoff(scanHost(scannedSession));

    handoff.check();

    await vi.waitFor(() => expect(mocks.kill).toHaveBeenCalledWith(scannedSession.id));
    handoff.dispose();
    forgetSession(scannedSession.id);
    vi.mocked(measuredContextPercentage).mockReturnValue(100);
    vi.mocked(compactionHeadroom).mockReturnValue(null);
  });

  it("clears its status line as soon as the session it waits on is closed", async () => {
    mocks.sendMessage.mockImplementation(async command => {
      if (command === Command.workspaceProbePath) {
        return { isFile: false };
      }

      return await answerMessage(command);
    });
    vi.mocked(sessionStatus).mockReturnValueOnce(SessionStatus.enum.ready).mockReturnValue(SessionStatus.enum.working);
    let sessions = [scannedSession];
    const handoff = createAutoHandoff({
      ...scanHost(scannedSession),
      sessions: () => sessions
    });

    vi.useFakeTimers();

    handoff.check();
    await vi.advanceTimersByTimeAsync(REQUEST_SUBMITTED_MS);
    expect(handoff.note).toContain("handing off to a fresh agent");
    sessions = [];
    await vi.advanceTimersByTimeAsync(CLOSED_SESSION_NOTICED_MS);

    expect(handoff.note).toBe("");
    expect(handoff.isHandingOff(scannedSession.id)).toBe(false);
    expect(mocks.kill).not.toHaveBeenCalled();
    handoff.dispose();
    vi.useRealTimers();
    forgetSession(scannedSession.id);
    vi.mocked(sessionStatus).mockReturnValue(SessionStatus.enum.ready);
  });
});

/** Past the request's paste-settle and submit check, into the wait for its doc. */
const REQUEST_SUBMITTED_MS = 10_000;
/** A couple of the doc wait's disk polls - far short of its own timeout. */
const CLOSED_SESSION_NOTICED_MS = 3_000;

const WAITING_NOTE =
  "Claude Code context at 100% - handing off once its running work finishes, or just before it auto-compacts…";

const scannedSession: AgentSession = {
  id: "mid-turn-session",
  agent: {
    id: "claude",
    label: "Claude Code",
    command: "claude",
    reordersBidi: true
  },
  cwd: "C:/repositories/pade",
  project: "C:/repositories/pade"
};

function scanHost(session: AgentSession) {
  return {
    sessions: () => [session],
    availableAgents: () => [session.agent],
    isOptedOut: () => false,
    thresholdPercentage: () => 60,
    slugSource: () => "pade",
    projectDirectory: () => "C:/repositories/pade",
    endSession: mocks.kill,
    removeSession() {},
    launchSuccessor: () => "successor-session"
  };
}
