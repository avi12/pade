import {
  chatsForProject,
  persistedChatsFromSessions,
  registryWithoutChat,
  registryWithProjectChats,
  sidebarProjects,
  type StoredChatRegistry
} from "@/lib/session/chat-registry";
import type { AgentSession } from "@/lib/types";
import { describe, expect, it } from "vitest";

const PROJECT = "C:\\repositories\\pade";
const OTHER_PROJECT = "C:\\repositories\\poll";

function session(id: string, extra: Partial<AgentSession> = {}): AgentSession {
  return {
    id,
    agent: {
      id: "claude",
      label: "Claude Code",
      command: "claude",
      reordersBidi: false
    },
    conversationId: `conversation-${id}`,
    ...extra
  };
}

function registry(): StoredChatRegistry {
  return {
    chats: [
      {
        id: "a",
        project: PROJECT,
        agent: session("a").agent,
        conversationId: "conversation-a",
        updatedAt: 1
      },
      {
        id: "b",
        project: OTHER_PROJECT,
        agent: session("b").agent,
        label: "Poll chat",
        updatedAt: 1
      }
    ],
    activeChatByProject: {
      [PROJECT]: "a"
    },
    lastProject: PROJECT
  };
}

describe("persistedChatsFromSessions", () => {
  it("maps sessions to chats and skips terminal editors", () => {
    const chats = persistedChatsFromSessions({
      project: PROJECT,
      sessions: [
        session("a"),
        session("editor-1", {
          agent: {
            id: "editor-vscode",
            label: "VS Code",
            command: "code",
            reordersBidi: false
          }
        })
      ],
      labels: {
        a: "Fix the sidebar"
      },
      now: 42
    });
    expect(chats).toHaveLength(1);
    expect(chats[0]?.label).toBe("Fix the sidebar");
    expect(chats[0]?.conversationId).toBe("conversation-a");
    expect(chats[0]?.updatedAt).toBe(42);
  });
});

describe("registryWithProjectChats", () => {
  it("replaces only that project's chats and records it as last", () => {
    const next = registryWithProjectChats({
      registry: registry(),
      project: PROJECT,
      chats: persistedChatsFromSessions({
        project: PROJECT,
        sessions: [session("c")],
        labels: {},
        now: 2
      }),
      activeChatId: "c"
    });
    expect(next.chats.map(chat => chat.id)).toEqual(["b", "c"]);
    expect(next.activeChatByProject[PROJECT]).toBe("c");
    expect(next.lastProject).toBe(PROJECT);
  });

  it("keeps stored chats when the replacement is empty", () => {
    const before = registry();
    expect(
      registryWithProjectChats({
        registry: before,
        project: PROJECT,
        chats: [],
        activeChatId: null
      })
    ).toBe(before);
  });
});

describe("registryWithoutChat", () => {
  it("removes the chat and its active pointer", () => {
    const next = registryWithoutChat({
      registry: registry(),
      project: PROJECT,
      chatId: "a"
    });
    expect(
      chatsForProject({
        registry: next,
        project: PROJECT
      })
    ).toEqual([]);
    expect(next.activeChatByProject[PROJECT]).toBeUndefined();
    expect(next.chats.map(chat => chat.id)).toEqual(["b"]);
  });
});

describe("sidebarProjects", () => {
  it("uses live sessions for the current project and the registry elsewhere", () => {
    const groups = sidebarProjects({
      registry: registry(),
      projectOrder: [PROJECT, OTHER_PROJECT],
      currentProject: PROJECT,
      liveSessions: [session("live-1")],
      liveLabels: {
        "live-1": "Live chat"
      },
      recordedConversations: () => []
    });
    expect(groups[0]?.chats).toEqual([{
      id: "live-1",
      project: PROJECT,
      label: "Live chat",
      agentId: "claude",
      isLive: true
    }]);
    expect(groups[1]?.chats[0]?.label).toBe("Poll chat");
    expect(groups[1]?.chats[0]?.isLive).toBe(false);
  });

  it("keeps a project with no chats as an empty group, once", () => {
    const groups = sidebarProjects({
      registry: registry(),
      projectOrder: [OTHER_PROJECT, OTHER_PROJECT, PROJECT],
      currentProject: "",
      liveSessions: [],
      liveLabels: {},
      recordedConversations: () => []
    });
    expect(groups.map(group => group.project)).toEqual([OTHER_PROJECT, PROJECT]);
  });

  it("lists recorded conversations below the chats, skipping ones a chat holds", () => {
    function recorded(conversationId: string, title: string | null) {
      return {
        agentId: "claude",
        conversationId,
        title,
        updatedAt: 1,
        resumeArguments: []
      };
    }

    const groups = sidebarProjects({
      registry: registry(),
      projectOrder: [PROJECT],
      currentProject: PROJECT,
      liveSessions: [session("live-1")],
      liveLabels: {},
      recordedConversations: () => [recorded("conversation-live-1", "Live"), recorded("old", null)]
    });
    expect(groups[0]?.chats.map(chat => chat.label)).toEqual(["Claude Code", "Untitled conversation"]);
    expect(groups[0]?.chats[1]?.resume?.conversationId).toBe("old");
  });
});
