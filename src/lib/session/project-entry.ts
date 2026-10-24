// Which chats a project shows when it is entered or returned to: its
// persisted chats restored, a recorded conversation resumed (or brought
// forward when a chat already runs it), a picked chat focused, and a fresh
// chat started only when one is wanted. The app shell lends the session
// actions and how it launches a fresh chat; this owns the decisions.

import { launchArgumentsFrom, restoreProjectChats, shouldStartFreshChat } from "@/lib/session/chat-restore";
import type { ChatRestoreHost } from "@/lib/session/chat-restore";
import type { AgentSession, OpenTarget, RecordedConversation } from "@/lib/types";

type RestoreLaunchOptions = Parameters<ChatRestoreHost["launch"]>[0];

/** A launch whose session id is optional: a resumed conversation gets a
 *  fresh session, a restored chat keeps its own. */
type ChatLaunchOptions = Omit<RestoreLaunchOptions, "sessionId"> & Partial<Pick<RestoreLaunchOptions, "sessionId">>;

/** What the app shell provides for entering a project. */
export interface ProjectEntryHost extends Omit<ChatRestoreHost, "launch"> {
  launch: (options: ChatLaunchOptions) => string;
  sessions: () => readonly Pick<AgentSession, "id" | "conversationId">[];
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

/** Reopen a recorded conversation: bring it forward when a chat already
 *  runs it, else launch its agent on it, titled the way the agent titled
 *  it. Nothing happens when its agent is not installed. */
export function resumeRecordedConversation({ host, conversation }: {
  host: ProjectEntryHost;
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
    conversationId: conversation.conversationId,
    args: launchArgumentsFrom(conversation.resumeArguments)
  });
  labelWithTitle({
    host,
    id,
    title: conversation.title
  });
}

/** The picked chat of `target` when it is still open, else none. */
function openPickedChat({ host, target }: {
  host: ProjectEntryHost;
  target: OpenTarget;
}) {
  const pickedChatId = target.chatId;
  if (pickedChatId === undefined || !host.isOpen(pickedChatId)) {
    return undefined;
  }

  return pickedChatId;
}

/** What a return to the open project does with its target: resume the
 *  picked conversation, bring the picked chat to the front, or launch a
 *  fresh chat beside the live ones when one was asked for. */
export function focusReturnedProject({ host, target }: {
  host: ProjectEntryHost;
  target: OpenTarget;
}) {
  if (target.resume !== undefined) {
    resumeRecordedConversation({
      host,
      conversation: target.resume
    });
    return;
  }

  const pickedChatId = openPickedChat({
    host,
    target
  });
  if (pickedChatId !== undefined) {
    host.select(pickedChatId);
    return;
  }

  if (target.newChat === true) {
    host.launchFreshChat({
      path: target.path,
      agentId: target.agent
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

/** Enter a project: restore its persisted chats, resume the picked
 *  conversation, and start a fresh chat as `shouldStartFreshChat` says. */
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
