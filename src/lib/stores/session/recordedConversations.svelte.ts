// The conversations each agent recorded for a project - its `/resume` list -
// read through the backend for the sidebar, keyed by the normalized project
// path. A failed read lists nothing rather than a stale or broken list.

import { Command, sendMessage } from "@/lib/ipc/messaging";
import type { RecordedConversation } from "@/lib/types";
import { normalizePath } from "@/lib/util/paths";
import { settledValue } from "@/lib/util/settled";

const conversationsByProject = $state<Record<string, RecordedConversation[]>>({});

export function recordedConversations(project: string) {
  return conversationsByProject[normalizePath(project)] ?? [];
}

export async function refreshRecordedConversations(projects: readonly string[]) {
  for (const project of projects) {
    conversationsByProject[normalizePath(project)] = await settledValue({
      promise: sendMessage(Command.agentConversations, { project }),
      fallback: []
    });
  }
}
