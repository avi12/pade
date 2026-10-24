// Bringing a project's persisted chats back as live sessions. Each chat
// resumes its own conversation: Claude through its pinned conversation id,
// the others through the resume args the backend resolves for their folder
// (`agent_resume_args`). The app shell lends the session actions; this owns
// the restore order and which chat ends up in front.

import { Command, sendMessage } from "@/lib/ipc/messaging";
import { chatsForProject } from "@/lib/session/chats/chat-registry";
import type { PersistedChat, StoredChatRegistry } from "@/lib/session/chats/chat-registry";
import type { Agent, ModelChoice } from "@/lib/types";
import { settledValue } from "@/lib/util/async/settled";

/** What the app shell provides for a restore. */
export interface ChatRestoreHost {
  registry: StoredChatRegistry;
  agents: readonly Agent[];
  launch: (options: {
    agent: Agent;
    project: string;
    cwd?: string;
    branch?: string;
    conversationId?: string;
    args?: string[];
    modelChoice?: ModelChoice;
    sessionId: string;
  }) => string;
  select: (id: string) => void;
  isOpen: (id: string) => boolean;
  setLabel: (target: {
    id: string;
    label: string;
  }) => void;
}

/** The arguments that reopen this chat's conversation, or none when its
 *  agent resumes through its pinned conversation id alone. A failed lookup
 *  falls back to none - the chat still opens, just fresh. */
async function resumeArgumentsFor({ chat, project, agent }: {
  chat: PersistedChat;
  project: string;
  agent: Agent;
}) {
  return await settledValue({
    promise: sendMessage(Command.agentResumeArgs, {
      command: agent.command,
      cwd: chat.cwd ?? project,
      conversationId: chat.conversationId
    }),
    fallback: []
  });
}

/** The launch args that reopen a conversation - none when it resumes
 *  through its conversation id alone. */
export function launchArgumentsFrom(resumeArguments: string[]) {
  if (resumeArguments.length === 0) {
    return undefined;
  }

  return resumeArguments;
}

/** Relaunch one persisted chat on its own conversation, beside whatever the
 *  window already runs. Its session id is kept, so the chat stays itself. */
export async function restoreChat({ host, project, chat }: {
  host: ChatRestoreHost;
  project: string;
  chat: PersistedChat;
}) {
  const agent = host.agents.find(candidate => candidate.id === chat.agent.id) ?? chat.agent;
  const resumeArguments = await resumeArgumentsFor({
    chat,
    project,
    agent
  });
  const restoredId = host.launch({
    agent,
    project,
    cwd: chat.cwd,
    branch: chat.branch,
    conversationId: chat.conversationId,
    args: launchArgumentsFrom(resumeArguments),
    modelChoice: chat.modelChoice,
    sessionId: chat.id
  });
  if (chat.label !== undefined) {
    host.setLabel({
      id: restoredId,
      label: chat.label
    });
  }
}

/** Whether entering a project also starts a fresh chat: when a first
 *  prompt was written for it, when one was explicitly asked for, or when
 *  there were no persisted chats to bring back. */
export function shouldStartFreshChat({ initialPrompt, wantNewChat, hasRestoredChats }: {
  initialPrompt?: string;
  wantNewChat: boolean;
  hasRestoredChats: boolean;
}) {
  return initialPrompt !== undefined || wantNewChat || !hasRestoredChats;
}

/** Restore every persisted chat of `project`, then bring the picked chat
 *  (or the one last in front) forward. False when the project has none. */
export async function restoreProjectChats({ host, project, chatId }: {
  host: ChatRestoreHost;
  project: string;
  chatId?: string;
}) {
  const chats = chatsForProject({
    registry: host.registry,
    project
  });
  if (chats.length === 0) {
    return false;
  }

  for (const chat of chats) {
    if (host.isOpen(chat.id)) {
      continue;
    }

    await restoreChat({
      host,
      project,
      chat
    });
  }

  const focusChatId = chatId ?? host.registry.activeChatByProject[project];
  if (focusChatId !== undefined && host.isOpen(focusChatId)) {
    host.select(focusChatId);
  }

  return true;
}
