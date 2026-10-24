// What a click on a chat row in the left rail does. Rows come in four kinds -
// a session live in this window, one live in another PADE window, a persisted
// chat nothing runs, a recorded conversation - and Ctrl, Cmd or Shift asks for
// a new window. One pure decision, so the rail, the shell and the tests agree.

import type { ElsewhereSession } from "@/lib/dashboard/agent-board";
import type { WindowHandoff } from "@/lib/shell/window-handoff";
import type { AgentSession, OpenTarget, RecordedConversation } from "@/lib/types";

/** A chat row picked in the rail. */
export interface ChatPick {
  project: string;
  chatId: string;
  opensNewWindow: boolean;
}

/** A recorded conversation picked in the rail. */
export interface ConversationPick {
  project: string;
  conversation: RecordedConversation;
  opensNewWindow: boolean;
}

/** The closed set of things a chat click can do. */
export const ChatClickAction = {
  /** Bring this window's live session forward. */
  SelectHere: "select-here",
  /** Focus the window that runs it and bring it forward there. */
  FocusOtherWindow: "focus-other-window",
  /** Move this window's live session - same process, still running - into a
   *  new window. */
  MoveToNewWindow: "move-to-new-window",
  /** Open a new window on its project that resumes it there. */
  ReopenInNewWindow: "reopen-in-new-window",
  /** Resume a recorded conversation in a new session here. */
  Resume: "resume",
  /** Relaunch a persisted chat on its conversation here. */
  Relaunch: "relaunch"
} as const;
type ChatClickAction = (typeof ChatClickAction)[keyof typeof ChatClickAction];

/** What a click on a chat row does. A session another window runs is always
 *  brought forward there - it already has its own window, so even a
 *  new-window click only focuses it. */
export function chatClickAction({ isLiveHere, elsewhereWindow, isRecorded, opensNewWindow }: {
  isLiveHere: boolean;
  /** The other window that runs it, if one does. */
  elsewhereWindow: string | undefined;
  isRecorded: boolean;
  opensNewWindow: boolean;
}): ChatClickAction {
  if (elsewhereWindow !== undefined && !isLiveHere) {
    return ChatClickAction.FocusOtherWindow;
  }

  if (opensNewWindow) {
    if (isLiveHere) {
      return ChatClickAction.MoveToNewWindow;
    }

    return ChatClickAction.ReopenInNewWindow;
  }

  if (isLiveHere) {
    return ChatClickAction.SelectHere;
  }

  if (isRecorded) {
    return ChatClickAction.Resume;
  }

  return ChatClickAction.Relaunch;
}

/** What the shell does on a chat click's behalf. */
export interface ChatClickHost {
  /** Bring this window's session forward. */
  select: (id: string) => void;
  /** Focus another window and bring its session forward there. */
  focusElsewhere: (target: {
    label: string;
    sessionId: string;
  }) => Promise<void>;
  moveToNewWindow: (session: AgentSession) => Promise<void>;
  reopenInNewWindow: (handoff: WindowHandoff) => Promise<void>;
  /** Resume or relaunch it here, beside the open sessions. */
  openHere: (target: OpenTarget) => Promise<void>;
}

/** Carry out a chat click: decide with `chatClickAction`, then run it. */
export async function runChatClick({ host, liveSession, elsewhere, isRecorded, opensNewWindow, reopen, openHere }: {
  host: ChatClickHost;
  /** This window's session running the chat, if any. */
  liveSession: AgentSession | undefined;
  /** Another window's session running it, if any. */
  elsewhere: Pick<ElsewhereSession, "id" | "windowLabel"> | undefined;
  isRecorded: boolean;
  opensNewWindow: boolean;
  /** The hand-off that reopens it in a new window. */
  reopen: WindowHandoff;
  /** The entry that resumes or relaunches it here. */
  openHere: OpenTarget;
}) {
  const handlers: Record<ChatClickAction, () => Promise<void>> = {
    async [ChatClickAction.SelectHere]() {
      if (liveSession !== undefined) {
        host.select(liveSession.id);
      }
    },
    async [ChatClickAction.FocusOtherWindow]() {
      if (elsewhere !== undefined) {
        await host.focusElsewhere({
          label: elsewhere.windowLabel,
          sessionId: elsewhere.id
        });
      }
    },
    async [ChatClickAction.MoveToNewWindow]() {
      if (liveSession !== undefined) {
        await host.moveToNewWindow(liveSession);
      }
    },
    [ChatClickAction.ReopenInNewWindow]: async () => await host.reopenInNewWindow(reopen),
    [ChatClickAction.Resume]: async () => await host.openHere(openHere),
    [ChatClickAction.Relaunch]: async () => await host.openHere(openHere)
  };
  await handlers[chatClickAction({
    isLiveHere: liveSession !== undefined,
    elsewhereWindow: elsewhere?.windowLabel,
    isRecorded,
    opensNewWindow
  })]();
}
