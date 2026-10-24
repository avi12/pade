import type { StoredChatRegistry } from "@/lib/session/chat-registry";
import { enterProjectChats, focusReturnedProject, resumeRecordedConversation } from "@/lib/session/project-entry";
import type { ProjectEntryHost } from "@/lib/session/project-entry";
import type { RecordedConversation } from "@/lib/types";
import { describe, expect, it, vi } from "vitest";

vi.mock("@/lib/ipc/messaging", async importOriginal => {
  const messaging = await importOriginal<typeof import("@/lib/ipc/messaging")>();
  return {
    ...messaging,
    sendMessage: vi.fn(async () => [])
  };
});

const PROJECT = "C:\\repositories\\pade";
const AGENT = {
  id: "claude",
  label: "Claude Code",
  command: "claude",
  reordersBidi: false
};
const FRESH_SESSION_ID = "fresh";

const EMPTY_REGISTRY: StoredChatRegistry = {
  chats: [],
  activeChatByProject: {},
  lastProject: ""
};

const PERSISTED_REGISTRY: StoredChatRegistry = {
  chats: [{
    id: "persisted",
    project: PROJECT,
    agent: AGENT,
    conversationId: "conversation-persisted",
    updatedAt: 1
  }],
  activeChatByProject: {},
  lastProject: PROJECT
};

function conversation(overrides: Partial<RecordedConversation> = {}): RecordedConversation {
  return {
    agentId: AGENT.id,
    conversationId: "conversation-recorded",
    title: "Fix the sidebar",
    updatedAt: 1,
    resumeArguments: [],
    ...overrides
  };
}

interface Launch {
  conversationId?: string;
  args?: string[];
  sessionId?: string;
}

function fakeHost({ registry = EMPTY_REGISTRY, running = [] }: {
  registry?: StoredChatRegistry;
  running?: {
    id: string;
    conversationId: string;
  }[];
} = {}) {
  const launches: Launch[] = [];
  const labels: {
    id: string;
    label: string;
  }[] = [];
  const selected: string[] = [];
  const freshChats: {
    path: string;
    agentId?: string;
    initialPrompt?: string;
  }[] = [];
  const host: ProjectEntryHost = {
    registry,
    agents: [AGENT],
    launch(options) {
      launches.push({
        conversationId: options.conversationId,
        args: options.args,
        sessionId: options.sessionId
      });
      return options.sessionId ?? FRESH_SESSION_ID;
    },
    select: id => selected.push(id),
    isOpen: id => running.some(session => session.id === id) || launches.some(launch => launch.sessionId === id),
    setLabel: target => labels.push(target),
    sessions: () => running,
    launchFreshChat: options => freshChats.push(options)
  };
  return {
    host,
    launches,
    labels,
    selected,
    freshChats
  };
}

describe("resumeRecordedConversation", () => {
  it("selects the chat already running the conversation instead of relaunching it", () => {
    const fake = fakeHost({
      running: [{
        id: "live",
        conversationId: "conversation-recorded"
      }]
    });
    resumeRecordedConversation({
      host: fake.host,
      conversation: conversation()
    });
    expect(fake.selected).toEqual(["live"]);
    expect(fake.launches).toEqual([]);
  });

  it("launches its agent on the conversation with its resume args, labelled with its title", () => {
    const fake = fakeHost();
    resumeRecordedConversation({
      host: fake.host,
      conversation: conversation({ resumeArguments: ["--resume", "conversation-recorded"] })
    });
    expect(fake.launches).toEqual([{
      conversationId: "conversation-recorded",
      args: ["--resume", "conversation-recorded"],
      sessionId: undefined
    }]);
    expect(fake.labels).toEqual([{
      id: FRESH_SESSION_ID,
      label: "Fix the sidebar"
    }]);
  });

  it("passes no args when the conversation has no resume args, and no label without a title", () => {
    const fake = fakeHost();
    resumeRecordedConversation({
      host: fake.host,
      conversation: conversation({ title: null })
    });
    expect(fake.launches).toEqual([{
      conversationId: "conversation-recorded",
      args: undefined,
      sessionId: undefined
    }]);
    expect(fake.labels).toEqual([]);
  });

  it("does nothing when the conversation's agent is not installed", () => {
    const fake = fakeHost();
    resumeRecordedConversation({
      host: fake.host,
      conversation: conversation({ agentId: "missing" })
    });
    expect(fake.launches).toEqual([]);
    expect(fake.selected).toEqual([]);
  });
});

describe("focusReturnedProject", () => {
  it("resumes a picked conversation", () => {
    const fake = fakeHost();
    focusReturnedProject({
      host: fake.host,
      target: {
        path: PROJECT,
        resume: conversation(),
        newChat: true
      }
    });
    expect(fake.launches).toHaveLength(1);
    expect(fake.freshChats).toEqual([]);
  });

  it("focuses a picked chat that is open", () => {
    const fake = fakeHost({
      running: [{
        id: "live",
        conversationId: "conversation-live"
      }]
    });
    focusReturnedProject({
      host: fake.host,
      target: {
        path: PROJECT,
        chatId: "live",
        newChat: true
      }
    });
    expect(fake.selected).toEqual(["live"]);
    expect(fake.freshChats).toEqual([]);
  });

  it("starts a new chat when asked and nothing else was picked", () => {
    const fake = fakeHost();
    focusReturnedProject({
      host: fake.host,
      target: {
        path: PROJECT,
        chatId: "closed",
        agent: AGENT.id,
        newChat: true
      }
    });
    expect(fake.selected).toEqual([]);
    expect(fake.freshChats).toEqual([{
      path: PROJECT,
      agentId: AGENT.id
    }]);
  });

  it("leaves the live chats alone on a plain return", () => {
    const fake = fakeHost();
    focusReturnedProject({
      host: fake.host,
      target: {
        path: PROJECT
      }
    });
    expect(fake.selected).toEqual([]);
    expect(fake.freshChats).toEqual([]);
    expect(fake.launches).toEqual([]);
  });
});

describe("enterProjectChats", () => {
  it("restores persisted chats without a fresh one", async () => {
    const fake = fakeHost({ registry: PERSISTED_REGISTRY });
    await enterProjectChats({
      host: fake.host,
      target: {
        path: PROJECT
      }
    });
    expect(fake.launches.map(launch => launch.sessionId)).toEqual(["persisted"]);
    expect(fake.freshChats).toEqual([]);
  });

  it("starts a fresh chat for a project with nothing to restore", async () => {
    const fake = fakeHost();
    await enterProjectChats({
      host: fake.host,
      target: {
        path: PROJECT,
        agent: AGENT.id
      }
    });
    expect(fake.freshChats).toEqual([{
      path: PROJECT,
      agentId: AGENT.id,
      initialPrompt: undefined
    }]);
  });

  it("skips the restore for a just-created project", async () => {
    const fake = fakeHost({ registry: PERSISTED_REGISTRY });
    await enterProjectChats({
      host: fake.host,
      target: {
        path: PROJECT,
        created: true
      }
    });
    expect(fake.launches).toEqual([]);
    expect(fake.freshChats).toHaveLength(1);
  });

  it("resumes a picked conversation in place of a fresh chat", async () => {
    const fake = fakeHost();
    await enterProjectChats({
      host: fake.host,
      target: {
        path: PROJECT,
        resume: conversation()
      }
    });
    expect(fake.launches.map(launch => launch.conversationId)).toEqual(["conversation-recorded"]);
    expect(fake.freshChats).toEqual([]);
  });

  it("adds a fresh chat beside the restored ones for a first prompt", async () => {
    const fake = fakeHost({ registry: PERSISTED_REGISTRY });
    await enterProjectChats({
      host: fake.host,
      target: {
        path: PROJECT,
        initialPrompt: "build it"
      }
    });
    expect(fake.launches).toHaveLength(1);
    expect(fake.freshChats).toEqual([{
      path: PROJECT,
      agentId: undefined,
      initialPrompt: "build it"
    }]);
  });
});
