import {
  layoutWithoutPane,
  layoutWithoutSession,
  layoutWithoutSessions,
  paneAtPoint,
  paneIdsAfterSplit,
  sessionsAfterReorder,
  wrappedIndex
} from "@/lib/session/pane-layout";
import { DropSide } from "@/lib/session/reorder";
import type { AgentSession } from "@/lib/types";
import { describe, expect, it } from "vitest";

const CLAUDE = {
  id: "claude",
  label: "Claude Code",
  command: "claude",
  reordersBidi: true
};

function session(id: string): AgentSession {
  return {
    id,
    agent: CLAUDE,
    conversationId: `conversation-${id}`
  };
}

function ids(sessions: AgentSession[]) {
  return sessions.map(each => each.id);
}

function rectangle({ left, width }: {
  left: number;
  width: number;
}) {
  return {
    left,
    right: left + width,
    top: 0,
    bottom: 100,
    width
  };
}

describe("paneAtPoint", () => {
  const panes = [
    {
      id: "a",
      rectangle: rectangle({
        left: 0,
        width: 100
      })
    },
    {
      id: "b",
      rectangle: rectangle({
        left: 100,
        width: 100
      })
    }
  ];

  it("finds the pane under the pointer and the half it is over", () => {
    expect(
      paneAtPoint({
        panes,
        x: 120,
        y: 50
      })
    ).toEqual({
      id: "b",
      side: DropSide.Left
    });
    expect(
      paneAtPoint({
        panes,
        x: 80,
        y: 50
      })
    ).toEqual({
      id: "a",
      side: DropSide.Right
    });
  });

  it("misses outside every pane", () => {
    expect(
      paneAtPoint({
        panes,
        x: 50,
        y: 150
      })
    ).toBeNull();
  });

  it("never targets a collapsed pane", () => {
    expect(
      paneAtPoint({
        panes: [{
          id: "a",
          rectangle: rectangle({
            left: 0,
            width: 0
          })
        }],
        x: 0,
        y: 50
      })
    ).toBeNull();
  });
});

describe("paneIdsAfterSplit", () => {
  it("inserts a new pane on the dropped side", () => {
    expect(
      paneIdsAfterSplit({
        paneIds: ["a", "b"],
        draggedId: "c",
        targetId: "a",
        side: DropSide.Right
      })
    ).toEqual(["a", "c", "b"]);
  });

  it("moves a pane that is already shown", () => {
    expect(
      paneIdsAfterSplit({
        paneIds: ["a", "b", "c"],
        draggedId: "a",
        targetId: "c",
        side: DropSide.Right
      })
    ).toEqual(["b", "c", "a"]);
  });
});

describe("sessionsAfterReorder", () => {
  it("reorders the visible prefix and leaves the overflow tail put", () => {
    const sessions = ["a", "b", "c", "d"].map(session);
    expect(
      ids(
        sessionsAfterReorder({
          sessions,
          orderedIds: ["b", "a"]
        })
      )
    ).toEqual(["b", "a", "c", "d"]);
  });
});

describe("wrappedIndex", () => {
  it("wraps at both ends", () => {
    expect(
      wrappedIndex({
        index: 2,
        delta: 1,
        count: 3
      })
    ).toBe(0);
    expect(
      wrappedIndex({
        index: 0,
        delta: -1,
        count: 3
      })
    ).toBe(2);
  });
});

describe("layoutWithoutPane", () => {
  it("hands the active pane to the last one still shown", () => {
    expect(
      layoutWithoutPane({
        layout: {
          sessions: ["a", "b", "c"].map(session),
          paneIds: ["a", "b", "c"],
          activeId: "b"
        },
        id: "b"
      })
    ).toMatchObject({
      paneIds: ["a", "c"],
      activeId: "c"
    });
  });
});

describe("layoutWithoutSession", () => {
  it("falls back to the last tab and shows it when the split empties", () => {
    const layout = layoutWithoutSession({
      layout: {
        sessions: ["a", "b"].map(session),
        paneIds: ["b"],
        activeId: "b"
      },
      id: "b"
    });
    expect(ids(layout.sessions)).toEqual(["a"]);
    expect(layout.paneIds).toEqual(["a"]);
    expect(layout.activeId).toBe("a");
  });

  it("keeps the active session when another one goes", () => {
    const layout = layoutWithoutSession({
      layout: {
        sessions: ["a", "b"].map(session),
        paneIds: ["a", "b"],
        activeId: "a"
      },
      id: "b"
    });
    expect(layout.paneIds).toEqual(["a"]);
    expect(layout.activeId).toBe("a");
  });

  it("empties out when the last session goes", () => {
    expect(
      layoutWithoutSession({
        layout: {
          sessions: [session("a")],
          paneIds: ["a"],
          activeId: "a"
        },
        id: "a"
      })
    ).toEqual({
      sessions: [],
      paneIds: [],
      activeId: null
    });
  });
});

describe("layoutWithoutSessions", () => {
  it("drops every listed session and falls back to the last remaining tab", () => {
    const layout = layoutWithoutSessions({
      layout: {
        sessions: ["a", "b", "c"].map(session),
        paneIds: ["b", "c"],
        activeId: "c"
      },
      ids: new Set(["b", "c"])
    });
    expect(ids(layout.sessions)).toEqual(["a"]);
    expect(layout.paneIds).toEqual([]);
    expect(layout.activeId).toBe("a");
  });
});
