import { activityCounts, ChatActivity, chatActivity, shownChats } from "@/lib/shell/sidebar/chats/chat-activity";
import { SessionStatus } from "@/lib/types";
import { describe, expect, it } from "vitest";

function chats(count: number) {
  return Array.from({ length: count }, (_, i) => ({ id: `chat-${i}` }));
}

describe("chatActivity", () => {
  it("reads a live agent at its prompt as waiting on the user", () => {
    expect(
      chatActivity({
        isLive: true,
        status: SessionStatus.enum.ready
      })
    ).toBe(ChatActivity.NeedsYou);
  });

  it("reads a starting or mid-turn agent as working", () => {
    expect(
      chatActivity({
        isLive: true,
        status: SessionStatus.enum.working
      })
    ).toBe(ChatActivity.Working);
    expect(
      chatActivity({
        isLive: true,
        status: SessionStatus.enum.starting
      })
    ).toBe(ChatActivity.Working);
  });

  it("reads anything not running as idle", () => {
    expect(
      chatActivity({
        isLive: false,
        status: SessionStatus.enum.ready
      })
    ).toBe(ChatActivity.Idle);
    expect(
      chatActivity({
        isLive: true,
        status: SessionStatus.enum.exited
      })
    ).toBe(ChatActivity.Idle);
  });
});

describe("activityCounts", () => {
  it("counts the working chats and the ones waiting on the user", () => {
    expect(
      activityCounts([
        ChatActivity.Working,
        ChatActivity.NeedsYou,
        ChatActivity.Idle,
        ChatActivity.NeedsYou
      ])
    ).toEqual({
      working: 1,
      needsYou: 2
    });
  });
});

describe("shownChats", () => {
  it("shows every chat while they fit", () => {
    expect(
      shownChats({
        chats: chats(3),
        activeChatId: null,
        isShowingAll: false,
        limit: 5
      })
    ).toEqual({
      shown: chats(3),
      hiddenCount: 0
    });
  });

  it("trims past the limit and counts the rest", () => {
    const shown = shownChats({
      chats: chats(8),
      activeChatId: null,
      isShowingAll: false,
      limit: 5
    });
    expect(shown.shown.map(chat => chat.id)).toEqual(["chat-0", "chat-1", "chat-2", "chat-3", "chat-4"]);
    expect(shown.hiddenCount).toBe(3);
  });

  it("shows a lone chat past the limit rather than a link to it", () => {
    expect(
      shownChats({
        chats: chats(6),
        activeChatId: null,
        isShowingAll: false,
        limit: 5
      }).hiddenCount
    ).toBe(0);
    expect(
      shownChats({
        chats: chats(7),
        activeChatId: "chat-6",
        isShowingAll: false,
        limit: 5
      }).hiddenCount
    ).toBe(0);
  });

  it("keeps the focused chat in view when it sits past the limit", () => {
    const shown = shownChats({
      chats: chats(8),
      activeChatId: "chat-6",
      isShowingAll: false,
      limit: 5
    });
    expect(shown.shown.map(chat => chat.id)).toContain("chat-6");
    expect(shown.hiddenCount).toBe(2);
  });

  it("shows everything once asked to", () => {
    expect(
      shownChats({
        chats: chats(8),
        activeChatId: null,
        isShowingAll: true,
        limit: 5
      }).hiddenCount
    ).toBe(0);
  });
});
