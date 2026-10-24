import {
  chatsForProject,
  persistedChatsFromSessions,
  registryWithLiveChats,
  registryWithoutChat,
  sidebarProjects,
  type StoredChatRegistry
} from "@/lib/session/chats/chat-registry";
import { SessionStatus } from "@/lib/types";
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
    project: PROJECT,
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
        conversationId: "conversation-b",
        updatedAt: 1
      }
    ],
    activeChatByProject: {
      [PROJECT]: "a"
    },
    lastProject: PROJECT
  };
}

function recorded(conversationId: string, title: string | null) {
  return {
    agentId: "claude",
    conversationId,
    title,
    updatedAt: 1,
    resumeArguments: []
  };
}

function elsewhere(id: string, extra: Partial<{
  project: string;
  conversationId: string;
}> = {}) {
  return {
    id,
    project: PROJECT,
    name: `Elsewhere ${id}`,
    agentId: "claude",
    conversationId: `conversation-${id}`,
    status: SessionStatus.enum.working,
    windowLabel: "w-2",
    ...extra
  };
}

describe("persistedChatsFromSessions", () => {
  it("maps sessions to chats and skips terminal editors", () => {
    const chats = persistedChatsFromSessions({
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

  it("files each session under its own project", () => {
    const chats = persistedChatsFromSessions({
      sessions: [session("a"), session("p", { project: OTHER_PROJECT })],
      labels: {},
      now: 1
    });
    expect(chats.map(chat => [chat.id, chat.project])).toEqual([
      ["a", PROJECT],
      ["p", OTHER_PROJECT]
    ]);
  });
});

describe("registryWithLiveChats", () => {
  it("upserts the live chats, keeps every other stored chat and records the front project", () => {
    const next = registryWithLiveChats({
      registry: registry(),
      chats: persistedChatsFromSessions({
        sessions: [session("c"), session("p", { project: OTHER_PROJECT })],
        labels: {},
        now: 2
      }),
      activeChatId: "p",
      activeProject: OTHER_PROJECT
    });
    expect(next.chats.map(chat => chat.id)).toEqual(["a", "b", "c", "p"]);
    expect(next.activeChatByProject[OTHER_PROJECT]).toBe("p");
    expect(next.activeChatByProject[PROJECT]).toBe("a");
    expect(next.lastProject).toBe(OTHER_PROJECT);
  });

  it("replaces a live chat's stored copy in place of adding a second", () => {
    const next = registryWithLiveChats({
      registry: registry(),
      chats: persistedChatsFromSessions({
        sessions: [session("a")],
        labels: {
          a: "Renamed"
        },
        now: 2
      }),
      activeChatId: "a",
      activeProject: PROJECT
    });
    expect(next.chats.filter(chat => chat.id === "a").map(chat => chat.label)).toEqual(["Renamed"]);
  });

  it("keeps the registry when nothing is live", () => {
    const before = registry();
    expect(
      registryWithLiveChats({
        registry: before,
        chats: [],
        activeChatId: null,
        activeProject: ""
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
  it("shows every project's live sessions here as live rows, its stored chats below", () => {
    const groups = sidebarProjects({
      registry: registry(),
      projectOrder: [PROJECT, OTHER_PROJECT],
      activeProject: PROJECT,
      liveSessions: [session("live-1"), session("live-2", { project: OTHER_PROJECT })],
      liveLabels: {
        "live-1": "Live chat"
      },
      elsewhereSessions: [],
      recordedConversations: () => []
    });
    expect(groups[0]?.isCurrent).toBe(true);
    expect(groups[0]?.chats.map(chat => [chat.id, chat.isLive])).toEqual([
      ["live-1", true],
      ["a", false]
    ]);
    expect(groups[0]?.chats[0]?.label).toBe("Live chat");
    expect(groups[1]?.isCurrent).toBe(false);
    expect(groups[1]?.chats.map(chat => [chat.id, chat.isLive])).toEqual([
      ["live-2", true],
      ["b", false]
    ]);
  });

  it("keeps a project with no chats as an empty group, once", () => {
    const groups = sidebarProjects({
      registry: registry(),
      projectOrder: [OTHER_PROJECT, OTHER_PROJECT, PROJECT],
      activeProject: "",
      liveSessions: [],
      liveLabels: {},
      elsewhereSessions: [],
      recordedConversations: () => []
    });
    expect(groups.map(group => group.project)).toEqual([OTHER_PROJECT, PROJECT]);
  });

  it("lists recorded conversations below the chats, skipping ones a chat holds", () => {
    const groups = sidebarProjects({
      registry: registry(),
      projectOrder: [PROJECT],
      activeProject: PROJECT,
      liveSessions: [session("live-1")],
      liveLabels: {},
      elsewhereSessions: [],
      recordedConversations: () => [
        recorded("conversation-live-1", "Live"),
        recorded("conversation-a", "Stored"),
        recorded("old", null)
      ]
    });
    expect(groups[0]?.chats.map(chat => chat.label)).toEqual(["Claude Code", "Claude Code", "Untitled conversation"]);
    expect(groups[0]?.chats[2]?.resume?.conversationId).toBe("old");
  });

  it("marks another window's sessions as live there and never offers them to resume", () => {
    const groups = sidebarProjects({
      registry: registry(),
      projectOrder: [PROJECT, OTHER_PROJECT],
      activeProject: PROJECT,
      liveSessions: [],
      liveLabels: {},
      elsewhereSessions: [elsewhere("a"), elsewhere("far", {
        project: OTHER_PROJECT,
        conversationId: "conversation-b"
      })],
      recordedConversations: project => project === PROJECT ? [recorded("conversation-a", "Running elsewhere")] : []
    });
    expect(groups[0]?.chats).toEqual([{
      id: "a",
      project: PROJECT,
      label: "Elsewhere a",
      agentId: "claude",
      isLive: true,
      windowLabel: "w-2",
      status: SessionStatus.enum.working
    }]);
    expect(groups[1]?.chats.map(chat => [chat.id, chat.windowLabel])).toEqual([["far", "w-2"]]);
  });

  it("prefers this window's own row when a session is reported here and elsewhere", () => {
    const groups = sidebarProjects({
      registry: registry(),
      projectOrder: [PROJECT],
      activeProject: PROJECT,
      liveSessions: [session("moved")],
      liveLabels: {},
      elsewhereSessions: [elsewhere("moved")],
      recordedConversations: () => []
    });
    expect(groups[0]?.chats.filter(chat => chat.id === "moved").map(chat => chat.windowLabel)).toEqual([undefined]);
  });
});
