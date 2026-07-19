import {
  clearSessionSnapshot,
  pruneToLive,
  readSessionSnapshot,
  saveSessionSnapshot,
  type SessionSnapshot
} from "@/lib/session-restore";
import type { AgentSession } from "@/lib/types";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

// A minimal sessionStorage double — vitest runs in node, which has none.
const backing = new Map<string, string>();
beforeEach(() => {
  backing.clear();
  vi.stubGlobal("sessionStorage", {
    getItem: (key: string) => backing.get(key) ?? null,
    setItem: (key: string, value: string) => void backing.set(key, value),
    removeItem: (key: string) => void backing.delete(key)
  });
});
afterEach(() => vi.unstubAllGlobals());

function session(id: string, extra: Partial<AgentSession> = {}): AgentSession {
  return {
    id,
    agent: {
      id: "claude",
      label: "Claude Code",
      command: "claude"
    },
    ...extra
  };
}

describe("save / read round trip", () => {
  it("persists the pane mapping and reads it back validated", () => {
    saveSessionSnapshot({
      project: "C:\\repos\\pade",
      sessions: [session("a"), session("b", { branch: "main" })],
      paneIds: ["a", "b"],
      activeId: "b"
    });

    const snapshot = readSessionSnapshot();
    expect(snapshot?.project).toBe("C:\\repos\\pade");
    expect(snapshot?.sessions.map(s => s.id)).toEqual(["a", "b"]);
    expect(snapshot?.sessions[1]?.branch).toBe("main");
    expect(snapshot?.paneIds).toEqual(["a", "b"]);
    expect(snapshot?.activeId).toBe("b");
  });

  it("strips the already-submitted initial prompt", () => {
    saveSessionSnapshot({
      project: "C:\\repos\\pade",
      sessions: [session("a", { initialPrompt: "build me a thing" })],
      paneIds: ["a"],
      activeId: "a"
    });

    const stored = backing.get("pade.session-snapshot") ?? "";
    expect(stored).not.toContain("build me a thing");
  });

  it("clears instead of saving when there is nothing to re-attach", () => {
    saveSessionSnapshot({
      project: "C:\\repos\\pade",
      sessions: [session("a")],
      paneIds: ["a"],
      activeId: "a"
    });

    saveSessionSnapshot({
      project: "C:\\repos\\pade",
      sessions: [],
      paneIds: [],
      activeId: null
    });
    expect(readSessionSnapshot()).toBeNull();

    saveSessionSnapshot({
      project: "",
      sessions: [session("a")],
      paneIds: ["a"],
      activeId: "a"
    });
    expect(readSessionSnapshot()).toBeNull();
  });

  it("reads null when the snapshot is absent, garbage, or the wrong shape", () => {
    expect(readSessionSnapshot()).toBeNull();

    backing.set("pade.session-snapshot", "{not json");
    expect(readSessionSnapshot()).toBeNull();

    backing.set("pade.session-snapshot", JSON.stringify({ project: "x" }));
    expect(readSessionSnapshot()).toBeNull();
  });

  it("clearSessionSnapshot forgets a saved snapshot", () => {
    saveSessionSnapshot({
      project: "C:\\repos\\pade",
      sessions: [session("a")],
      paneIds: ["a"],
      activeId: "a"
    });
    clearSessionSnapshot();
    expect(readSessionSnapshot()).toBeNull();
  });
});

describe("pruneToLive", () => {
  const snapshot: SessionSnapshot = {
    project: "C:\\repos\\pade",
    sessions: [session("a"), session("b"), session("c")],
    paneIds: ["a", "b"],
    activeId: "b"
  };

  it("keeps only the sessions the backend still hosts", () => {
    const pruned = pruneToLive({
      snapshot,
      liveIds: new Set(["a", "c"])
    });
    expect(pruned?.sessions.map(s => s.id)).toEqual(["a", "c"]);
  });

  it("prunes dead panes and re-points the active id at a survivor", () => {
    const pruned = pruneToLive({
      snapshot,
      liveIds: new Set(["a", "c"])
    });
    expect(pruned?.paneIds).toEqual(["a"]);
    expect(pruned?.activeId).toBe("a");
  });

  it("keeps the layout untouched when every session survived", () => {
    const pruned = pruneToLive({
      snapshot,
      liveIds: new Set(["a", "b", "c"])
    });
    expect(pruned?.paneIds).toEqual(["a", "b"]);
    expect(pruned?.activeId).toBe("b");
  });

  it("shows the first survivor when no shown pane survived", () => {
    const pruned = pruneToLive({
      snapshot,
      liveIds: new Set(["c"])
    });
    expect(pruned?.paneIds).toEqual(["c"]);
    expect(pruned?.activeId).toBe("c");
  });

  it("is null when nothing survived — what a deliberate leave looks like", () => {
    expect(pruneToLive({
      snapshot,
      liveIds: new Set()
    })).toBeNull();
  });
});
