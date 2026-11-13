// What each chat in the left rail is doing at a glance - working, waiting on
// the user, or idle - and the counts and the trimmed list the rail builds from
// it. One home for the mapping, so the header's totals, a project's count
// boxes and a chat row's dot can never disagree.

import type { SidebarChat } from "@/lib/session/chats/chat-registry";
import { sessionStatus } from "@/lib/stores/session/sessions.svelte";
import { SessionStatus } from "@/lib/types/agent";

export const ChatActivity = {
  Working: "working",
  NeedsYou: "needs-you",
  Idle: "idle"
} as const;
export type ChatActivity = (typeof ChatActivity)[keyof typeof ChatActivity];

/** What a chat waiting on the user says, on its row and in the rail's head. */
export const NEEDS_YOU = "needs you";

/** How many chats a project lists before the rest wait behind "Show N more". */
const SHOWN_CHAT_LIMIT = 5;

/** A chat's status: the one another window reported, else this window's own
 *  reading (reactive). */
export function chatStatus(chat: Pick<SidebarChat, "id" | "status">) {
  return chat.status ?? sessionStatus(chat.id);
}

/** What a chat is doing: a live agent at its prompt waits on the user, one
 *  starting up or mid-turn is working, and anything not running is idle. */
export function chatActivity({ isLive, status }: { isLive: boolean; status: SessionStatus }) {
  if (!isLive || status === SessionStatus.enum.exited) {
    return ChatActivity.Idle;
  }

  if (status === SessionStatus.enum.ready) {
    return ChatActivity.NeedsYou;
  }

  return ChatActivity.Working;
}

/** How many of `activities` are working and how many wait on the user. */
export function activityCounts(activities: Iterable<ChatActivity>) {
  let working = 0;
  let needsYou = 0;
  for (const activity of activities) {
    if (activity === ChatActivity.Working) {
      working += 1;
    }

    if (activity === ChatActivity.NeedsYou) {
      needsYou += 1;
    }
  }

  return {
    working,
    needsYou
  };
}

/** The chats a project shows: the first `limit`, plus the focused one when it
 *  sits further down (so the chat in front never hides), or every chat once
 *  the user asked for all of them. A single chat past the limit shows rather
 *  than hide behind a "Show 1 more" link as tall as itself. */
export function shownChats<Chat extends Pick<SidebarChat, "id">>({
  chats,
  activeChatId,
  isShowingAll,
  limit = SHOWN_CHAT_LIMIT
}: {
  chats: readonly Chat[];
  activeChatId: string | null;
  isShowingAll: boolean;
  limit?: number;
}) {
  const isWorthTrimming = chats.length > limit + 1;
  if (isShowingAll || !isWorthTrimming) {
    return {
      shown: chats,
      hiddenCount: 0
    };
  }

  const shown = chats.filter((chat, i) => i < limit || chat.id === activeChatId);
  const isOnlyOneHidden = chats.length - shown.length === 1;
  if (isOnlyOneHidden) {
    return {
      shown: chats,
      hiddenCount: 0
    };
  }

  return {
    shown,
    hiddenCount: chats.length - shown.length
  };
}
