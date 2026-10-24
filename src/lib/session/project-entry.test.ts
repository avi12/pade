import type { StoredChatRegistry } from "@/lib/session/chat-registry";
import {
  enterProjectChats,
  joinProjectChats,
  JoinStep,
  joinStep,
  resumeRecordedConversation
} from "@/lib/session/project-entry";
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
const OTHER_PROJECT = "C:\\repositories\\poll";
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
  project: string;
  conversationId?: string;
  args?: string[];
  sessionId?: string;
}

interface RunningSession {
  id: string;
  conversationId: string;
  project: string;
}

function fakeHost({ registry = EMPTY_REGISTRY, running = [] }: {
  registry?: StoredChatRegistry;
  running?: RunningSession[];
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
        project: options.project,
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
        conversationId: "conversation-recorded",
        project: PROJECT
      }]
    });
    resumeRecordedConversation({
      host: fake.host,
      project: PROJECT,
      conversation: conversation()
    });
    expect(fake.selected).toEqual(["live"]);
    expect(fake.launches).toEqual([]);
  });

  it("launches its agent on the conversation in its project, labelled with its title", () => {
    const fake = fakeHost();
    resumeRecordedConversation({
      host: fake.host,
      project: OTHER_PROJECT,
      conversation: conversation({ resumeArguments: ["--resume", "conversation-recorded"] })
    });
    expect(fake.launches).toEqual([{
      project: OTHER_PROJECT,
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
      project: PROJECT,
      conversation: conversation({ title: null })
    });
    expect(fake.launches).toEqual([{
      project: PROJECT,
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
      project: PROJECT,
      conversation: conversation({ agentId: "missing" })
    });
    expect(fake.launches).toEqual([]);
    expect(fake.selected).toEqual([]);
  });
});

describe("joinStep", () => {
  const NOTHING = {
    isPickedChatOpen: false,
    isPickedChatPersisted: false,
    hasLiveSession: false
  };

  it("resumes a picked conversation before anything else", () => {
    expect(
      joinStep({
        ...NOTHING,
        target: {
          path: PROJECT,
          resume: conversation(),
          newChat: true
        }
      })
    ).toBe(JoinStep.Resume);
  });

  it("brings an open picked chat forward, and relaunches a persisted one", () => {
    expect(
      joinStep({
        ...NOTHING,
        isPickedChatOpen: true,
        target: {
          path: PROJECT,
          chatId: "live"
        }
      })
    ).toBe(JoinStep.SelectChat);
    expect(
      joinStep({
        ...NOTHING,
        isPickedChatPersisted: true,
        target: {
          path: PROJECT,
          chatId: "stored"
        }
      })
    ).toBe(JoinStep.RelaunchChat);
  });

  it("starts a fresh chat on a project click even when the project already runs one", () => {
    expect(
      joinStep({
        ...NOTHING,
        hasLiveSession: true,
        target: {
          path: PROJECT,
          newChat: true
        }
      })
    ).toBe(JoinStep.FreshChat);
  });

  it("brings a live session forward on a plain open, else starts one", () => {
    expect(
      joinStep({
        ...NOTHING,
        hasLiveSession: true,
        target: {
          path: PROJECT
        }
      })
    ).toBe(JoinStep.SelectLive);
    expect(
      joinStep({
        ...NOTHING,
        target: {
          path: PROJECT
        }
      })
    ).toBe(JoinStep.FreshChat);
  });
});

describe("joinProjectChats", () => {
  const OTHER_RUNNING: RunningSession = {
    id: "other",
    conversationId: "conversation-other",
    project: OTHER_PROJECT
  };

  it("starts a fresh chat beside another project's sessions, stopping nothing", async () => {
    const fake = fakeHost({ running: [OTHER_RUNNING] });
    await joinProjectChats({
      host: fake.host,
      target: {
        path: PROJECT
      }
    });
    expect(fake.freshChats).toEqual([{
      path: PROJECT,
      agentId: undefined,
      initialPrompt: undefined
    }]);
    expect(fake.selected).toEqual([]);
  });

  it("relaunches only the picked persisted chat, in its project", async () => {
    const fake = fakeHost({
      registry: PERSISTED_REGISTRY,
      running: [OTHER_RUNNING]
    });
    await joinProjectChats({
      host: fake.host,
      target: {
        path: PROJECT,
        chatId: "persisted"
      }
    });
    expect(fake.launches).toEqual([{
      project: PROJECT,
      conversationId: "conversation-persisted",
      args: undefined,
      sessionId: "persisted"
    }]);
    expect(fake.freshChats).toEqual([]);
  });

  it("brings the project's own live session forward on a plain open", async () => {
    const fake = fakeHost({
      running: [OTHER_RUNNING, {
        id: "mine",
        conversationId: "conversation-mine",
        project: PROJECT
      }]
    });
    await joinProjectChats({
      host: fake.host,
      target: {
        path: PROJECT
      }
    });
    expect(fake.selected).toEqual(["mine"]);
    expect(fake.freshChats).toEqual([]);
  });

  it("resumes a recorded conversation into its project", async () => {
    const fake = fakeHost({ running: [OTHER_RUNNING] });
    await joinProjectChats({
      host: fake.host,
      target: {
        path: PROJECT,
        resume: conversation()
      }
    });
    expect(fake.launches.map(launch => [launch.project, launch.conversationId])).toEqual([[PROJECT, "conversation-recorded"]]);
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
