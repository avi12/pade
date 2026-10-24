import {
  HandoffKind,
  handoffPayload,
  handoffProject,
  joinTargetOf,
  mergeMenuItemLabel,
  mergeTargets,
  parseHandoffs
} from "@/lib/shell/handoff/window-handoff";
import type { WindowHandoff } from "@/lib/shell/handoff/window-handoff";
import { describe, expect, it, vi } from "vitest";

const PADE = "C:\\repositories\\pade";
const POLL = "C:\\repositories\\poll";
const AGENT = {
  id: "claude",
  label: "Claude Code",
  command: "claude",
  reordersBidi: false
};

const SESSIONS_HANDOFF: WindowHandoff = {
  kind: HandoffKind.enum.sessions,
  sessions: [
    {
      id: "a",
      agent: AGENT,
      project: PADE,
      label: "Fix the rail"
    },
    {
      id: "b",
      agent: AGENT,
      project: POLL
    }
  ],
  activeId: "b"
};

function windowRow(label: string, path: string, isCurrent = false) {
  return {
    label,
    path,
    isCurrent
  };
}

describe("hand-off payloads", () => {
  it("round-trip through their wire form", () => {
    expect(parseHandoffs([handoffPayload(SESSIONS_HANDOFF)])).toEqual([SESSIONS_HANDOFF]);
  });

  it("drop a malformed payload rather than trust it", () => {
    vi.spyOn(console, "warn").mockImplementation(() => undefined);
    const conversation: WindowHandoff = {
      kind: HandoffKind.enum.conversation,
      project: PADE,
      conversation: {
        agentId: "claude",
        conversationId: "c-1",
        title: null,
        updatedAt: 1,
        resumeArguments: []
      }
    };
    expect(
      parseHandoffs(["not json", JSON.stringify({
        kind: "sessions",
        sessions: []
      }), handoffPayload(conversation)])
    ).toEqual([conversation]);
  });

  it("land in the project of the session brought forward", () => {
    expect(handoffProject(SESSIONS_HANDOFF)).toBe(POLL);
    expect(
      handoffProject({
        kind: HandoffKind.enum.chat,
        project: PADE,
        chatId: "c"
      })
    ).toBe(PADE);
  });
});

describe("joinTargetOf", () => {
  it("enters a handed conversation or chat the way a rail click does", () => {
    const conversation = {
      agentId: "claude",
      conversationId: "c-1",
      title: null,
      updatedAt: 1,
      resumeArguments: []
    };
    expect(
      joinTargetOf({
        kind: HandoffKind.enum.conversation,
        project: PADE,
        conversation
      })
    ).toEqual({
      path: PADE,
      resume: conversation
    });
    expect(
      joinTargetOf({
        kind: HandoffKind.enum.chat,
        project: POLL,
        chatId: "chat-1"
      })
    ).toEqual({
      path: POLL,
      chatId: "chat-1"
    });
  });
});

describe("merging this window into another", () => {
  it("offers every other window, and nothing while this one is alone", () => {
    expect(mergeTargets([windowRow("main", PADE, true)])).toEqual([]);
    expect(mergeTargets([windowRow("main", PADE, true), windowRow("w-1", POLL)]).map(target => target.label)).toEqual(["w-1"]);
  });

  it("names the one other window in the menu item, and skips the item for several", () => {
    expect(
      mergeMenuItemLabel({
        windowRows: [windowRow("main", PADE, true), windowRow("w-1", POLL)],
        labels: {}
      })
    ).toBe("Merge into repositories/poll");
    expect(
      mergeMenuItemLabel({
        windowRows: [windowRow("main", PADE, true), windowRow("w-1", POLL), windowRow("w-2", PADE)],
        labels: {}
      })
    ).toBeNull();
    expect(
      mergeMenuItemLabel({
        windowRows: [windowRow("main", PADE, true)],
        labels: {}
      })
    ).toBeNull();
  });
});
