import {
  clearSessionSnapshot,
  pruneToLive,
  readSessionSnapshot,
  saveSessionSnapshot,
  type SessionSnapshot
} from "@/lib/session-restore";
import type { AgentSession } from "@/lib/types";
import {
  afterEach,
  beforeEach,
  describe,
  expect,
  it,
  vi
} from "vitest";

// Minimal localStorage and Tauri-window doubles — vitest runs in node, which
// has neither. The label is what the snapshot key is scoped to.
const backing = new Map<string, string>();
const KEY = "pade.session-snapshot:main";
function stubWindowLabel(label: string) {
  vi.stubGlobal("window", {
    __TAURI_INTERNALS__: {
      metadata: {
        currentWindow: {
          label
        }
      }
    }
  });
}
beforeEach(() => {
  backing.clear();
  stubWindowLabel("main");
  vi.stubGlobal("localStorage", {
    getItem: (key: string) => backing.get(key) ?? null,
    setItem(key: string, value: string) {
      backing.set(key, value);
    },
    removeItem(key: string) {
      backing.delete(key);
    }
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
    expect(snapshot?.sessions.map(session => session.id)).toEqual(["a", "b"]);
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

    const stored = backing.get(KEY) ?? "";
    expect(stored).not.toContain("build me a thing");
  });

  it("keys the snapshot by window label, so a crash-rebuilt window finds its own", () => {
    saveSessionSnapshot({
      project: "C:\\repos\\pade",
      sessions: [session("a")],
      paneIds: ["a"],
      activeId: "a"
    });
    expect([...backing.keys()]).toEqual([KEY]);

    // A sibling window reads nothing of this one's — and rebuilding the crashed
    // window (same label, fresh webview) still finds the mapping, which is what
    // sessionStorage lost.
    stubWindowLabel("w-2");
    expect(readSessionSnapshot()).toBeNull();
    stubWindowLabel("main");
    expect(readSessionSnapshot()?.sessions[0]?.id).toBe("a");
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

    backing.set(KEY, "{not json");
    expect(readSessionSnapshot()).toBeNull();

    backing.set(KEY, JSON.stringify({ project: "x" }));
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
    expect(pruned?.sessions.map(session => session.id)).toEqual(["a", "c"]);
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
    expect(
      pruneToLive({
        snapshot,
        liveIds: new Set()
      })
    ).toBeNull();
  });
});
