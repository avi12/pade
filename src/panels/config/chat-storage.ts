// The chat-storage limits the Config panel offers, each with the label its
// option prints, and the words for what a limit would delete.

import { CHAT_SIZE_LIMIT_CHOICES, CHAT_STORAGE_LIMIT_CHOICES } from "@/lib/types";
import type { Agent, ChatStorage, DeletedChat } from "@/lib/types";
import {
  BYTES_PER_MEGABYTE,
  formatAge,
  formatBytes,
  formatMegabytes,
  formatQuantity
} from "@/lib/util/format";
import { baseName } from "@/lib/util/paths";

/** How a chat its agent never titled is listed. */
const UNTITLED_CHAT = "Untitled chat";

function limitChoices(megabytes: readonly (number | null)[]) {
  return megabytes.map(value => ({
    value,
    label: value === null ? "Off" : formatMegabytes(value)
  }));
}

/** How big one chat may grow - or Off, any size. */
export const CHAT_SIZE_CHOICES = limitChoices(CHAT_SIZE_LIMIT_CHOICES);

/** How much all chats together may take - or Off, no total. */
export const CHAT_STORAGE_CHOICES = limitChoices(CHAT_STORAGE_LIMIT_CHOICES);

/** The two limits as saved, one of them about to change. */
export interface ChatStorageLimits {
  sizeLimitMegabytes: number | null;
  storageLimitMegabytes: number | null;
}

/** "12.7 GB in 1,234 chats". */
export function storageSummary(storage: ChatStorage) {
  return `${formatBytes(storage.bytes)} in ${formatQuantity({
    count: storage.chatCount,
    noun: "chat"
  })}`;
}

/** Each agent's share, largest first, by the name the picker shows it under
 *  ("Claude Code 12.3 GB · Codex 139 MB"). */
export function agentShares({ storage, agents }: {
  storage: ChatStorage;
  agents: readonly Agent[];
}) {
  return storage.agents
    .map(share => `${agentName({
      agentId: share.agentId,
      agents
    })} ${formatBytes(share.bytes)}`)
    .join(" · ");
}

/** Why the chats stay over the total limit after a cleanup, when they do:
 *  last week's chats and each project's newest are never deleted. `null`
 *  when the total fits, or there is no total limit. */
export function keptAboveLimit({ storage, storageLimitMegabytes }: {
  storage: ChatStorage;
  storageLimitMegabytes: number | null;
}) {
  if (storageLimitMegabytes === null) {
    return null;
  }

  const keptBytes = storage.bytes - storage.cleanup.bytes;
  const isOverLimit = keptBytes > storageLimitMegabytes * BYTES_PER_MEGABYTE;
  if (!isOverLimit) {
    return null;
  }

  return `Recent chats, and each project's newest, still take ${formatBytes(keptBytes)}, so the total stays above ${formatMegabytes(storageLimitMegabytes)} until they're a week old`;
}

function agentName({ agentId, agents }: {
  agentId: string;
  agents: readonly Agent[];
}) {
  return agents.find(agent => agent.id === agentId)?.label ?? agentId;
}

/** One deleted chat as the confirmation lists it: its title, and which agent
 *  and folder it belongs to, its size and when it was last used ("Claude Code
 *  · edge-plus · 206 MB · 10d ago"). */
export function deletedChatRow({ chat, agents, now }: {
  chat: DeletedChat;
  agents: readonly Agent[];
  now: number;
}) {
  const details = [
    agentName({
      agentId: chat.agentId,
      agents
    }),
    chat.project === null ? null : baseName(chat.project),
    formatBytes(chat.bytes),
    `${formatAge({
      stamp: chat.updatedAt,
      now
    })} ago`
  ];
  return {
    title: chat.title ?? UNTITLED_CHAT,
    details: details.filter(detail => detail !== null).join(" · ")
  };
}

/** The deleted chats the list leaves out - it names the largest - when it
 *  can't name them all ("And 22,914 smaller chats"). `null` when every one
 *  is listed. */
export function unlistedChats(cleanup: ChatStorage["cleanup"]) {
  const unlisted = cleanup.chatCount - cleanup.chats.length;
  if (unlisted <= 0) {
    return null;
  }

  return `And ${formatQuantity({
    count: unlisted,
    noun: "smaller chat"
  })}`;
}

/** What confirming a limit deletes right away ("412 chats, 10.8 GB"). */
export function cleanupSummary(cleanup: ChatStorage["cleanup"]) {
  return `${formatQuantity({
    count: cleanup.chatCount,
    noun: "chat"
  })}, ${formatBytes(cleanup.bytes)}`;
}
