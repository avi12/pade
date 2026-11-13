// What a click on a chat row in the left rail does. Rows come in four kinds -
// a session live in this window, one live in another PADE window, a persisted
// chat nothing runs, a recorded conversation - and Ctrl, Cmd or Shift asks for
// a new window. One pure decision, so the rail, the shell and the tests agree.

import type { ElsewhereSession } from "@/lib/dashboard/agent-board";
import type { WindowHandoff } from "@/lib/shell/handoff/window-handoff";
import type { AgentSession } from "@/lib/types/agent";
import type { RecordedConversation } from "@/lib/types/chat-storage";
import type { OpenTarget } from "@/lib/types/window";

/** A chat row picked in the rail. */
export interface ChatPick {
  project: string;
  chatId: string;
  shouldOpenNewWindow: boolean;
}

/** A recorded conversation picked in the rail. */
export interface ConversationPick {
  project: string;
  conversation: RecordedConversation;
  shouldOpenNewWindow: boolean;
}

/** The closed set of things a chat click can do. */
export const ChatClickAction = {
  /** Bring this window's live session forward. */
  SELECT_HERE: "select-here",
  /** Focus the window that runs it and bring it forward there. */
  FOCUS_OTHER_WINDOW: "focus-other-window",
  /** Move this window's live session - same process, still running - into a
   *  new window. */
  MOVE_TO_NEW_WINDOW: "move-to-new-window",
  /** Open a new window on its project that resumes it there. */
  REOPEN_IN_NEW_WINDOW: "reopen-in-new-window",
  /** Resume a recorded conversation in a new session here. */
  RESUME: "resume",
  /** Relaunch a persisted chat on its conversation here. */
  RELAUNCH: "relaunch"
} as const;
type ChatClickAction = (typeof ChatClickAction)[keyof typeof ChatClickAction];

/** What a click on a chat row does. A session another window runs is always
 *  brought forward there - it already has its own window, so even a
 *  new-window click only focuses it. */
export function chatClickAction({
  isLiveHere,
  elsewhereWindow,
  isRecorded,
  shouldOpenNewWindow
}: {
  isLiveHere: boolean;
  /** The other window that runs it, if one does. */
  elsewhereWindow: string | undefined;
  isRecorded: boolean;
  shouldOpenNewWindow: boolean;
}) {
  if (elsewhereWindow !== undefined && !isLiveHere) {
    return ChatClickAction.FOCUS_OTHER_WINDOW;
  }

  if (shouldOpenNewWindow) {
    if (isLiveHere) {
      return ChatClickAction.MOVE_TO_NEW_WINDOW;
    }

    return ChatClickAction.REOPEN_IN_NEW_WINDOW;
  }

  if (isLiveHere) {
    return ChatClickAction.SELECT_HERE;
  }

  if (isRecorded) {
    return ChatClickAction.RESUME;
  }

  return ChatClickAction.RELAUNCH;
}

/** What the shell does on a chat click's behalf. */
export interface ChatClickHost {
  /** Bring this window's session forward. */
  select: (id: string) => void;
  /** Focus another window and bring its session forward there. */
  focusElsewhere: (target: { label: string; sessionId: string }) => Promise<void>;
  moveToNewWindow: (session: AgentSession) => Promise<void>;
  reopenInNewWindow: (handoff: WindowHandoff) => Promise<void>;
  /** Resume or relaunch it here, beside the open sessions. */
  openHere: (target: OpenTarget) => Promise<void>;
}

/** Carry out a chat click: decide with `chatClickAction`, then run it. */
export async function runChatClick({
  host,
  liveSession,
  elsewhere,
  isRecorded,
  shouldOpenNewWindow,
  reopen,
  openHere
}: {
  host: ChatClickHost;
  /** This window's session running the chat, if any. */
  liveSession: AgentSession | undefined;
  /** Another window's session running it, if any. */
  elsewhere: Pick<ElsewhereSession, "id" | "windowLabel"> | undefined;
  isRecorded: boolean;
  shouldOpenNewWindow: boolean;
  /** The hand-off that reopens it in a new window. */
  reopen: WindowHandoff;
  /** The entry that resumes or relaunches it here. */
  openHere: OpenTarget;
}) {
  const handlers: Record<ChatClickAction, () => Promise<void>> = {
    async [ChatClickAction.SELECT_HERE]() {
      if (liveSession !== undefined) {
        host.select(liveSession.id);
      }
    },
    async [ChatClickAction.FOCUS_OTHER_WINDOW]() {
      if (elsewhere !== undefined) {
        await host.focusElsewhere({
          label: elsewhere.windowLabel,
          sessionId: elsewhere.id
        });
      }
    },
    async [ChatClickAction.MOVE_TO_NEW_WINDOW]() {
      if (liveSession !== undefined) {
        await host.moveToNewWindow(liveSession);
      }
    },
    [ChatClickAction.REOPEN_IN_NEW_WINDOW]: async () => await host.reopenInNewWindow(reopen),
    [ChatClickAction.RESUME]: async () => await host.openHere(openHere),
    [ChatClickAction.RELAUNCH]: async () => await host.openHere(openHere)
  };
  await handlers[
    chatClickAction({
      isLiveHere: liveSession !== undefined,
      elsewhereWindow: elsewhere?.windowLabel,
      isRecorded,
      shouldOpenNewWindow
    })
  ]();
}
