import { describe, expect, it, vi } from "vitest";

import { HandoffKind } from "@/lib/shell/handoff/window-handoff";
import { ChatClickAction, chatClickAction, runChatClick } from "@/lib/shell/sidebar/chats/chat-click";
import type { ChatClickHost } from "@/lib/shell/sidebar/chats/chat-click";
import type { AgentSession } from "@/lib/types/agent";

const PROJECT = "C:\\repositories\\pade";
const LIVE: AgentSession = {
  id: "live",
  agent: {
    id: "claude",
    label: "Claude Code",
    command: "claude",
    reordersBidi: false
  },
  project: PROJECT
};

function fakeHost() {
  return {
    select: vi.fn(),
    focusElsewhere: vi.fn(async () => undefined),
    moveToNewWindow: vi.fn(async () => undefined),
    reopenInNewWindow: vi.fn(async () => undefined),
    openHere: vi.fn(async () => undefined)
  } satisfies ChatClickHost;
}

function click({
  host,
  liveSession,
  elsewhere,
  shouldOpenNewWindow = false
}: {
  host: ChatClickHost;
  liveSession?: AgentSession;
  elsewhere?: {
    id: string;
    windowLabel: string;
  };
  shouldOpenNewWindow?: boolean;
}) {
  return runChatClick({
    host,
    liveSession,
    elsewhere,
    isRecorded: false,
    shouldOpenNewWindow,
    reopen: {
      kind: HandoffKind.enum.chat,
      project: PROJECT,
      chatId: "live"
    },
    openHere: {
      path: PROJECT,
      chatId: "live"
    }
  });
}

describe("runChatClick", () => {
  it("selects a live session here", async () => {
    const host = fakeHost();
    await click({
      host,
      liveSession: LIVE
    });
    expect(host.select).toHaveBeenCalledWith("live");
  });

  it("focuses the window that runs it, on its session", async () => {
    const host = fakeHost();
    await click({
      host,
      elsewhere: {
        id: "far",
        windowLabel: "w-2"
      },
      shouldOpenNewWindow: true
    });
    expect(host.focusElsewhere).toHaveBeenCalledWith({
      label: "w-2",
      sessionId: "far"
    });
  });

  it("moves a live session on a new-window click, and reopens a stored one", async () => {
    const host = fakeHost();
    await click({
      host,
      liveSession: LIVE,
      shouldOpenNewWindow: true
    });
    expect(host.moveToNewWindow).toHaveBeenCalledWith(LIVE);

    await click({
      host,
      shouldOpenNewWindow: true
    });
    expect(host.reopenInNewWindow).toHaveBeenCalledWith({
      kind: HandoffKind.enum.chat,
      project: PROJECT,
      chatId: "live"
    });
  });

  it("relaunches a stored chat here", async () => {
    const host = fakeHost();
    await click({ host });
    expect(host.openHere).toHaveBeenCalledWith({
      path: PROJECT,
      chatId: "live"
    });
  });
});

const PLAIN = {
  isLiveHere: false,
  elsewhereWindow: undefined,
  isRecorded: false,
  shouldOpenNewWindow: false
};

describe("chatClickAction", () => {
  it("selects a session live in this window", () => {
    expect(
      chatClickAction({
        ...PLAIN,
        isLiveHere: true
      })
    ).toBe(ChatClickAction.SELECT_HERE);
  });

  it("focuses the window that runs a session, even on a new-window click", () => {
    expect(
      chatClickAction({
        ...PLAIN,
        elsewhereWindow: "w-2"
      })
    ).toBe(ChatClickAction.FOCUS_OTHER_WINDOW);
    expect(
      chatClickAction({
        ...PLAIN,
        elsewhereWindow: "w-2",
        shouldOpenNewWindow: true
      })
    ).toBe(ChatClickAction.FOCUS_OTHER_WINDOW);
  });

  it("moves a live session to a new window on a new-window click", () => {
    expect(
      chatClickAction({
        ...PLAIN,
        isLiveHere: true,
        shouldOpenNewWindow: true
      })
    ).toBe(ChatClickAction.MOVE_TO_NEW_WINDOW);
  });

  it("reopens a persisted chat or a recorded conversation in a new window on a new-window click", () => {
    expect(
      chatClickAction({
        ...PLAIN,
        shouldOpenNewWindow: true
      })
    ).toBe(ChatClickAction.REOPEN_IN_NEW_WINDOW);
    expect(
      chatClickAction({
        ...PLAIN,
        isRecorded: true,
        shouldOpenNewWindow: true
      })
    ).toBe(ChatClickAction.REOPEN_IN_NEW_WINDOW);
  });

  it("resumes a recorded conversation and relaunches a persisted chat here", () => {
    expect(
      chatClickAction({
        ...PLAIN,
        isRecorded: true
      })
    ).toBe(ChatClickAction.RESUME);
    expect(chatClickAction(PLAIN)).toBe(ChatClickAction.RELAUNCH);
  });

  it("prefers this window's live copy over another window's report of it", () => {
    expect(
      chatClickAction({
        ...PLAIN,
        isLiveHere: true,
        elsewhereWindow: "w-2"
      })
    ).toBe(ChatClickAction.SELECT_HERE);
  });
});
