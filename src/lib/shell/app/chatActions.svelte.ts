import type { createBoardRoster } from "@/lib/dashboard/boardRoster.svelte";
import type { ChatRegistry } from "@/lib/shell/app/chatRegistry.svelte";
import type { ProjectFlow } from "@/lib/shell/app/projectFlow.svelte";
import type { ScreenPhase } from "@/lib/shell/app/screenPhase.svelte";
import { HandoffKind } from "@/lib/shell/handoff/window-handoff";
import type { createWindowHandoffs } from "@/lib/shell/handoff/windowHandoffs.svelte";
import type WorkspaceScreen from "@/lib/shell/screen/WorkspaceScreen.svelte";
import { runChatClick } from "@/lib/shell/sidebar/chats/chat-click";
import type { ChatClickHost, ChatPick, ConversationPick } from "@/lib/shell/sidebar/chats/chat-click";
import type { WindowSessions } from "@/lib/shell/windowSessions.svelte";
import type { AgentSession } from "@/lib/types/agent";

/** The left rail's actions on chats: switch, start, resume, stop and delete. */
export function createChatActions({
  windowSessions,
  roster,
  handoffs,
  screen,
  registry,
  projectFlow,
  workspaceScreen
}: {
  windowSessions: WindowSessions;
  roster: ReturnType<typeof createBoardRoster>;
  handoffs: ReturnType<typeof createWindowHandoffs>;
  screen: ScreenPhase;
  registry: ChatRegistry;
  projectFlow: ProjectFlow;
  /** The workspace screen while it is mounted - closing through its tab animates out. */
  workspaceScreen: () => WorkspaceScreen | undefined;
}) {
  // Through its tab while the workspace is on screen, so it animates out.
  async function closeThroughTab(session: AgentSession) {
    const screenInFront = workspaceScreen();
    if (screenInFront) {
      screenInFront.closeSession(session);
      return;
    }

    await windowSessions.close(session);
  }

  // A chat click does what lib/shell/sidebar/chats/chat-click decides: bring a live
  // session forward here or in the window that runs it, relaunch a persisted
  // chat or resume a recorded conversation beside the open sessions, or - on a
  // new-window click - move a live session (still running) or reopen the chat
  // in a new window. Deleting forgets the chat first, then closes it if it is
  // live here.
  const chatClickHost: ChatClickHost = {
    select(id) {
      windowSessions.select(id);
      void screen.showWorkspace();
    },
    focusElsewhere: roster.focus,
    moveToNewWindow: handoffs.moveToNewWindow,
    reopenInNewWindow: handoffs.reopenInNewWindow,
    openHere: projectFlow.openProject
  };

  async function selectChat({ project, chatId, shouldOpenNewWindow }: ChatPick) {
    await runChatClick({
      host: chatClickHost,
      liveSession: windowSessions.sessions.find(session => session.id === chatId),
      elsewhere: roster.sessions.find(session => session.id === chatId),
      isRecorded: false,
      shouldOpenNewWindow,
      reopen: {
        kind: HandoffKind.enum.chat,
        project,
        chatId
      },
      openHere: {
        path: project,
        chatId
      }
    });
  }

  // A recorded conversation picked in the left rail - the same decision, keyed
  // on the conversation a session runs.
  async function resumeConversation({ project, conversation, shouldOpenNewWindow }: ConversationPick) {
    function runsConversation(session: { conversationId?: string }) {
      return session.conversationId === conversation.conversationId;
    }

    await runChatClick({
      host: chatClickHost,
      liveSession: windowSessions.sessions.find(runsConversation),
      elsewhere: roster.sessions.find(runsConversation),
      isRecorded: true,
      shouldOpenNewWindow,
      reopen: {
        kind: HandoffKind.enum.conversation,
        project,
        conversation
      },
      openHere: {
        path: project,
        resume: conversation
      }
    });
  }

  async function deleteChat({ project, chatId }: { project: string; chatId: string }) {
    registry.forgetChat({
      project,
      chatId
    });

    const liveSession = windowSessions.sessions.find(session => session.id === chatId);
    if (liveSession !== undefined) {
      await windowSessions.close(liveSession);
    }
  }

  // The rail's stop button: end a live chat the way its tab's close does. It
  // lives on as a recorded conversation it can be resumed from.
  async function stopChat({ chatId }: { chatId: string }) {
    const liveSession = windowSessions.sessions.find(session => session.id === chatId);
    if (liveSession !== undefined) {
      await closeThroughTab(liveSession);
    }
  }

  return {
    closeThroughTab,
    selectChat,
    resumeConversation,
    deleteChat,
    stopChat
  };
}
