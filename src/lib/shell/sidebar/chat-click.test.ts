import { HandoffKind } from "@/lib/shell/handoff/window-handoff";
import { ChatClickAction, chatClickAction, runChatClick } from "@/lib/shell/sidebar/chat-click";
import type { ChatClickHost } from "@/lib/shell/sidebar/chat-click";
import type { AgentSession } from "@/lib/types";
import { describe, expect, it, vi } from "vitest";

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

function click({ host, liveSession, elsewhere, opensNewWindow = false }: {
  host: ChatClickHost;
  liveSession?: AgentSession;
  elsewhere?: {
    id: string;
    windowLabel: string;
  };
  opensNewWindow?: boolean;
}) {
  return runChatClick({
    host,
    liveSession,
    elsewhere,
    isRecorded: false,
    opensNewWindow,
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
      opensNewWindow: true
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
      opensNewWindow: true
    });
    expect(host.moveToNewWindow).toHaveBeenCalledWith(LIVE);

    await click({
      host,
      opensNewWindow: true
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
  opensNewWindow: false
};

describe("chatClickAction", () => {
  it("selects a session live in this window", () => {
    expect(
      chatClickAction({
        ...PLAIN,
        isLiveHere: true
      })
    ).toBe(ChatClickAction.SelectHere);
  });

  it("focuses the window that runs a session, even on a new-window click", () => {
    expect(
      chatClickAction({
        ...PLAIN,
        elsewhereWindow: "w-2"
      })
    ).toBe(ChatClickAction.FocusOtherWindow);
    expect(
      chatClickAction({
        ...PLAIN,
        elsewhereWindow: "w-2",
        opensNewWindow: true
      })
    ).toBe(ChatClickAction.FocusOtherWindow);
  });

  it("moves a live session to a new window on a new-window click", () => {
    expect(
      chatClickAction({
        ...PLAIN,
        isLiveHere: true,
        opensNewWindow: true
      })
    ).toBe(ChatClickAction.MoveToNewWindow);
  });

  it("reopens a persisted chat or a recorded conversation in a new window on a new-window click", () => {
    expect(
      chatClickAction({
        ...PLAIN,
        opensNewWindow: true
      })
    ).toBe(ChatClickAction.ReopenInNewWindow);
    expect(
      chatClickAction({
        ...PLAIN,
        isRecorded: true,
        opensNewWindow: true
      })
    ).toBe(ChatClickAction.ReopenInNewWindow);
  });

  it("resumes a recorded conversation and relaunches a persisted chat here", () => {
    expect(
      chatClickAction({
        ...PLAIN,
        isRecorded: true
      })
    ).toBe(ChatClickAction.Resume);
    expect(chatClickAction(PLAIN)).toBe(ChatClickAction.Relaunch);
  });

  it("prefers this window's live copy over another window's report of it", () => {
    expect(
      chatClickAction({
        ...PLAIN,
        isLiveHere: true,
        elsewhereWindow: "w-2"
      })
    ).toBe(ChatClickAction.SelectHere);
  });
});
