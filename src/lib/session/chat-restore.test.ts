import type { StoredChatRegistry } from "@/lib/session/chat-registry";
import { restoreProjectChats, shouldStartFreshChat } from "@/lib/session/chat-restore";
import type { ChatRestoreHost } from "@/lib/session/chat-restore";
import { describe, expect, it, vi } from "vitest";

vi.mock("@/lib/ipc/messaging", async importOriginal => {
  const messaging = await importOriginal<typeof import("@/lib/ipc/messaging")>();
  return {
    ...messaging,
    sendMessage: vi.fn(async () => ["resume", "conversation-a"])
  };
});

const PROJECT = "C:\\repositories\\pade";
const AGENT = {
  id: "claude",
  label: "Claude Code",
  command: "claude",
  reordersBidi: false
};

function registry(): StoredChatRegistry {
  return {
    chats: [{
      id: "a",
      project: PROJECT,
      agent: AGENT,
      label: "Fix the sidebar",
      conversationId: "conversation-a",
      updatedAt: 1
    }],
    activeChatByProject: {
      [PROJECT]: "a"
    },
    lastProject: PROJECT
  };
}

function host(chats: StoredChatRegistry) {
  const launched: string[] = [];
  const labels: {
    id: string;
    label: string;
  }[] = [];
  const selected: string[] = [];
  const restoreHost: ChatRestoreHost = {
    registry: chats,
    agents: [AGENT],
    launch: options => {
      launched.push(options.sessionId);
      return options.sessionId;
    },
    select: id => selected.push(id),
    isOpen: id => launched.includes(id),
    setLabel: target => labels.push(target)
  };
  return {
    restoreHost,
    launched,
    labels,
    selected
  };
}

describe("shouldStartFreshChat", () => {
  it("starts fresh only for a prompt, an explicit ask, or nothing restored", () => {
    expect(
      shouldStartFreshChat({
        wantNewChat: false,
        hasRestoredChats: true
      })
    ).toBe(false);
    expect(
      shouldStartFreshChat({
        initialPrompt: "build it",
        wantNewChat: false,
        hasRestoredChats: true
      })
    ).toBe(true);
    expect(
      shouldStartFreshChat({
        wantNewChat: true,
        hasRestoredChats: true
      })
    ).toBe(true);
    expect(
      shouldStartFreshChat({
        wantNewChat: false,
        hasRestoredChats: false
      })
    ).toBe(true);
  });
});

describe("restoreProjectChats", () => {
  it("relaunches each chat on its conversation and focuses the last active one", async () => {
    const fake = host(registry());
    const restored = await restoreProjectChats({
      host: fake.restoreHost,
      project: PROJECT
    });
    expect(restored).toBe(true);
    expect(fake.launched).toEqual(["a"]);
    expect(fake.labels).toEqual([{
      id: "a",
      label: "Fix the sidebar"
    }]);
    expect(fake.selected).toEqual(["a"]);
  });

  it("focuses the picked chat over the last active one", async () => {
    const fake = host(registry());
    await restoreProjectChats({
      host: fake.restoreHost,
      project: PROJECT,
      chatId: "a"
    });
    expect(fake.selected).toEqual(["a"]);
  });

  it("restores nothing for a project with no chats", async () => {
    const fake = host({
      chats: [],
      activeChatByProject: {},
      lastProject: ""
    });
    const restored = await restoreProjectChats({
      host: fake.restoreHost,
      project: PROJECT
    });
    expect(restored).toBe(false);
    expect(fake.launched).toEqual([]);
  });
});
