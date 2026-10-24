// The conversations each agent recorded for a project - its `/resume` list -
// read through the backend for the sidebar, keyed by the normalized project
// path. A failed read lists nothing rather than a stale or broken list. The
// store owns when to re-read: whenever the listed projects are handed in, and
// whenever the backend's watch reports an agent's store changed for one of
// them (a conversation started, renamed, grown or handed off).

import { Command, EventType, onMessage, sendMessage } from "@/lib/ipc/messaging";
import { projectsToReread } from "@/lib/stores/session/conversation-reread";
import type { RecordedConversation } from "@/lib/types";
import { normalizePath } from "@/lib/util/paths";
import { settledValue } from "@/lib/util/settled";

const conversationsByProject = $state<Record<string, RecordedConversation[]>>({});

// The projects on screen, so a reported change re-reads only what is shown.
let listedProjects: readonly string[] = [];
let isListening = false;
// The newest read asked for each project: an older read that lands after it
// is dropped, so a burst of changes never leaves the list behind.
const latestReads: Record<string, number> = {};

export function recordedConversations(project: string) {
  return conversationsByProject[normalizePath(project)] ?? [];
}

/** Show `projects`' recorded conversations: read them now, and again whenever
 *  an agent's store reports a change to one of them. */
export async function refreshRecordedConversations(projects: readonly string[]) {
  listedProjects = projects;
  await listenForConversationChanges();
  await rereadConversations(projects);
}

async function listenForConversationChanges() {
  if (isListening) {
    return;
  }

  isListening = true;
  await onMessage(EventType.agentConversationsChanged, async ({ data }) => {
    await rereadConversations(
      projectsToReread({
        listed: listedProjects,
        changedProject: data.project
      })
    );
  });
}

async function rereadConversations(projects: readonly string[]) {
  for (const project of projects) {
    await readProjectConversations(project);
  }
}

async function readProjectConversations(project: string) {
  const key = normalizePath(project);
  const read = (latestReads[key] ?? 0) + 1;
  latestReads[key] = read;
  const conversations = await settledValue({
    promise: sendMessage(Command.agentConversations, { project }),
    fallback: []
  });
  const isNewestRead = latestReads[key] === read;
  if (isNewestRead) {
    conversationsByProject[key] = conversations;
  }
}
