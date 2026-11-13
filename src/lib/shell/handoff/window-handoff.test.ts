import { describe, expect, it, vi } from "vitest";

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
      state: {
        label: "Fix the rail"
      }
    },
    {
      id: "b",
      agent: AGENT,
      project: POLL
    }
  ],
  activeId: "b"
};

function windowRow({ label, path, isCurrent = false }: { label: string; path: string; isCurrent?: boolean }) {
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
      parseHandoffs([
        "not json",
        JSON.stringify({
          kind: "sessions",
          sessions: []
        }),
        handoffPayload(conversation)
      ])
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
    expect(mergeTargets([windowRow({ label: "main", path: PADE, isCurrent: true })])).toEqual([]);
    expect(
      mergeTargets([
        windowRow({ label: "main", path: PADE, isCurrent: true }),
        windowRow({ label: "w-1", path: POLL })
      ]).map(target => target.label)
    ).toEqual(["w-1"]);
  });

  it("names the one other window in the menu item, and skips the item for several", () => {
    expect(
      mergeMenuItemLabel({
        windowRows: [
          windowRow({ label: "main", path: PADE, isCurrent: true }),
          windowRow({ label: "w-1", path: POLL })
        ],
        labels: {}
      })
    ).toBe("Merge into repositories\\poll");
    expect(
      mergeMenuItemLabel({
        windowRows: [
          windowRow({ label: "main", path: PADE, isCurrent: true }),
          windowRow({ label: "w-1", path: POLL }),
          windowRow({ label: "w-2", path: PADE })
        ],
        labels: {}
      })
    ).toBeNull();
    expect(
      mergeMenuItemLabel({
        windowRows: [windowRow({ label: "main", path: PADE, isCurrent: true })],
        labels: {}
      })
    ).toBeNull();
  });
});
