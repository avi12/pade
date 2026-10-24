// Which chats a project shows when it is entered: its persisted chats
// restored into an empty window, or - in a window already running sessions,
// which a project entry never stops - one session started beside them: a
// recorded conversation resumed (or brought forward when a chat already runs
// it), a picked chat focused or relaunched, a fresh chat, or the project's own
// live session brought forward. The app shell lends the session actions and
// how it launches a fresh chat; this owns the decisions.

import { persistedChat } from "@/lib/session/chats/chat-registry";
import {
  launchArgumentsFrom,
  restoreChat,
  restoreProjectChats,
  shouldStartFreshChat
} from "@/lib/session/chats/chat-restore";
import type { ChatRestoreHost } from "@/lib/session/chats/chat-restore";
import { sessionToFocus } from "@/lib/session/chats/session-projects";
import type { AgentSession, OpenTarget, RecordedConversation } from "@/lib/types";

type RestoreLaunchOptions = Parameters<ChatRestoreHost["launch"]>[0];

/** A launch whose session id is optional: a resumed conversation gets a
 *  fresh session, a restored chat keeps its own. */
type ChatLaunchOptions = Omit<RestoreLaunchOptions, "sessionId"> & Partial<Pick<RestoreLaunchOptions, "sessionId">>;

/** What the app shell provides for entering a project. */
export interface ProjectEntryHost extends Omit<ChatRestoreHost, "launch"> {
  launch: (options: ChatLaunchOptions) => string;
  sessions: () => readonly Pick<AgentSession, "id" | "conversationId" | "project">[];
  launchFreshChat: (options: {
    path: string;
    agentId?: string;
    initialPrompt?: string;
  }) => void;
}

function labelWithTitle({ host, id, title }: {
  host: ProjectEntryHost;
  id: string;
  title: string | null;
}) {
  if (title === null) {
    return;
  }

  host.setLabel({
    id,
    label: title
  });
}

/** Reopen a recorded conversation of `project`: bring it forward when a chat
 *  already runs it, else launch its agent on it, titled the way the agent
 *  titled it. Nothing happens when its agent is not installed. */
export function resumeRecordedConversation({ host, project, conversation }: {
  host: ProjectEntryHost;
  project: string;
  conversation: RecordedConversation;
}) {
  const runningChat = host.sessions().find(session => session.conversationId === conversation.conversationId);
  if (runningChat !== undefined) {
    host.select(runningChat.id);
    return;
  }

  const agent = host.agents.find(candidate => candidate.id === conversation.agentId);
  if (agent === undefined) {
    return;
  }

  const id = host.launch({
    agent,
    project,
    conversationId: conversation.conversationId,
    args: launchArgumentsFrom(conversation.resumeArguments)
  });
  labelWithTitle({
    host,
    id,
    title: conversation.title
  });
}

/** What entering a project beside running sessions does with its target. */
export const JoinStep = {
  Resume: "resume",
  SelectChat: "select-chat",
  RelaunchChat: "relaunch-chat",
  FreshChat: "fresh-chat",
  SelectLive: "select-live"
} as const;
type JoinStep = (typeof JoinStep)[keyof typeof JoinStep];

/** The one step entering `target` takes in a window that already runs
 *  sessions: resume the picked conversation; bring the picked chat forward,
 *  or relaunch it when it is persisted but not running; start a fresh chat
 *  when one was asked for (a new-chat click, a first prompt) or the project
 *  has nothing running here; else bring its live session forward. */
export function joinStep({ target, isPickedChatOpen, isPickedChatPersisted, hasLiveSession }: {
  target: OpenTarget;
  isPickedChatOpen: boolean;
  isPickedChatPersisted: boolean;
  hasLiveSession: boolean;
}): JoinStep {
  if (target.resume !== undefined) {
    return JoinStep.Resume;
  }

  if (target.chatId !== undefined && isPickedChatOpen) {
    return JoinStep.SelectChat;
  }

  if (target.chatId !== undefined && isPickedChatPersisted) {
    return JoinStep.RelaunchChat;
  }

  const wantsFreshChat = target.newChat === true || target.initialPrompt !== undefined || !hasLiveSession;
  if (wantsFreshChat) {
    return JoinStep.FreshChat;
  }

  return JoinStep.SelectLive;
}

/** Enter a project in a window that already runs sessions - from any project
 *  - without stopping any of them: one session is started or brought forward
 *  as `joinStep` decides, and the window's project context follows it. */
export async function joinProjectChats({ host, target }: {
  host: ProjectEntryHost;
  target: OpenTarget;
}) {
  const pickedChat = target.chatId === undefined
    ? undefined
    : persistedChat({
      registry: host.registry,
      project: target.path,
      chatId: target.chatId
    });
  const focusId = sessionToFocus({
    sessions: host.sessions(),
    project: target.path,
    lastActiveId: host.registry.activeChatByProject[target.path]
  });
  const step = joinStep({
    target,
    isPickedChatOpen: target.chatId !== undefined && host.isOpen(target.chatId),
    isPickedChatPersisted: pickedChat !== undefined,
    hasLiveSession: focusId !== undefined
  });
  if (step === JoinStep.Resume && target.resume !== undefined) {
    resumeRecordedConversation({
      host,
      project: target.path,
      conversation: target.resume
    });
  } else if (step === JoinStep.SelectChat && target.chatId !== undefined) {
    host.select(target.chatId);
  } else if (step === JoinStep.RelaunchChat && pickedChat !== undefined) {
    await restoreChat({
      host,
      project: target.path,
      chat: pickedChat
    });
  } else if (step === JoinStep.SelectLive && focusId !== undefined) {
    host.select(focusId);
  } else {
    host.launchFreshChat({
      path: target.path,
      agentId: target.agent,
      initialPrompt: target.initialPrompt
    });
  }
}

/** Bring back the project's persisted chats - none for a project that was
 *  just created. True when any came back. */
async function restoreExistingChats({ host, target }: {
  host: ProjectEntryHost;
  target: OpenTarget;
}) {
  if (target.created === true) {
    return false;
  }

  return await restoreProjectChats({
    host,
    project: target.path,
    chatId: target.chatId
  });
}

/** Enter a project in an empty window: restore its persisted chats, resume
 *  the picked conversation, and start a fresh chat as `shouldStartFreshChat`
 *  says. */
export async function enterProjectChats({ host, target }: {
  host: ProjectEntryHost;
  target: OpenTarget;
}) {
  const hasRestoredChats = await restoreExistingChats({
    host,
    target
  });
  if (target.resume !== undefined) {
    resumeRecordedConversation({
      host,
      project: target.path,
      conversation: target.resume
    });
  }

  const wantsFreshChat = shouldStartFreshChat({
    initialPrompt: target.initialPrompt,
    wantNewChat: target.newChat === true,
    hasRestoredChats: hasRestoredChats || target.resume !== undefined
  });
  if (wantsFreshChat) {
    host.launchFreshChat({
      path: target.path,
      agentId: target.agent,
      initialPrompt: target.initialPrompt
    });
  }
}
