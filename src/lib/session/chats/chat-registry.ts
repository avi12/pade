// The persisted chat registry: every project's chats, kept in localStorage
// so the left sidebar can list them, switch between them and bring them back
// after a relaunch. A chat is a session that has been closed with the app -
// its process is gone, but its agent, conversation id and label remain, so
// reopening it resumes that conversation (Claude through its pinned
// `--session-id`, the others through `agent_resume_args`).

import type { ElsewhereSession } from "@/lib/dashboard/agent-board";
import { isSameProject, sessionsOfProject } from "@/lib/session/chats/session-projects";
import { Agent, ModelChoice } from "@/lib/types";
import type { AgentSession, RecordedConversation, SessionStatus } from "@/lib/types";
import { normalizePath } from "@/lib/util/paths";
import { readStoredJson, writeStoredJson } from "@/lib/util/stored-json";
import { z } from "zod";

const CHAT_REGISTRY_STORAGE_KEY = "pade.project-chats";

/** Terminal-editor sessions carry this agent id prefix - they are editors
 *  opened in a pane, not chats, so they never enter the registry. */
const EDITOR_AGENT_PREFIX = "editor-";

/** One chat as it survives a relaunch: enough to relaunch the same agent on
 *  the same conversation in the same folder. */
const PersistedChat = z.object({
  id: z.string().min(1),
  project: z.string().min(1),
  agent: Agent,
  label: z.string().optional(),
  cwd: z.string().optional(),
  branch: z.string().optional(),
  conversationId: z.string().optional(),
  modelChoice: ModelChoice.optional(),
  updatedAt: z.number()
});
export type PersistedChat = z.infer<typeof PersistedChat>;

/** The whole registry: every persisted chat, which chat each project last had
 *  in front, and the project the user last worked in. */
const StoredChatRegistry = z.object({
  chats: z.array(PersistedChat).default([]),
  activeChatByProject: z.record(z.string(), z.string()).default({}),
  lastProject: z.string().default("")
});
export type StoredChatRegistry = z.infer<typeof StoredChatRegistry>;

function emptyChatRegistry(): StoredChatRegistry {
  return {
    chats: [],
    activeChatByProject: {},
    lastProject: ""
  };
}

export function loadChatRegistry(): StoredChatRegistry {
  return readStoredJson({
    key: CHAT_REGISTRY_STORAGE_KEY,
    schema: StoredChatRegistry
  }) ?? emptyChatRegistry();
}

export function saveChatRegistry(registry: StoredChatRegistry) {
  writeStoredJson({
    key: CHAT_REGISTRY_STORAGE_KEY,
    value: registry
  });
}

/** The window's live sessions as persisted chats, each under the project it
 *  belongs to. Editor panes are skipped - they are not conversations. */
export function persistedChatsFromSessions({ sessions, labels, now }: {
  sessions: readonly AgentSession[];
  labels: Readonly<Record<string, string | undefined>>;
  now: number;
}): PersistedChat[] {
  const chats: PersistedChat[] = [];
  for (const session of sessions) {
    if (session.agent.id.startsWith(EDITOR_AGENT_PREFIX)) {
      continue;
    }

    chats.push({
      id: session.id,
      project: session.project,
      agent: session.agent,
      label: labels[session.id],
      cwd: session.cwd,
      branch: session.branch,
      conversationId: session.conversationId,
      modelChoice: session.modelChoice,
      updatedAt: now
    });
  }

  return chats;
}

/** Store the window's live chats, each under its own project: a live chat
 *  replaces its stored copy, every other stored chat stays (a chat closed for
 *  good is forgotten through `registryWithoutChat`, never by being absent - a
 *  window leaving a project passes through empty, and a chat relaunched from
 *  the rail must not erase its project's other chats). The project in front
 *  records its active chat and becomes the last project. No live chats leaves
 *  the registry as it is. */
export function registryWithLiveChats({ registry, chats, activeChatId, activeProject }: {
  registry: StoredChatRegistry;
  chats: readonly PersistedChat[];
  activeChatId: string | null;
  activeProject: string;
}): StoredChatRegistry {
  if (chats.length === 0) {
    return registry;
  }

  const liveIds = new Set(chats.map(chat => chat.id));
  const isActiveChatLive = activeChatId !== null && liveIds.has(activeChatId);
  return {
    chats: [...registry.chats.filter(chat => !liveIds.has(chat.id)), ...chats],
    activeChatByProject: isActiveChatLive
      ? {
        ...registry.activeChatByProject,
        [activeProject]: activeChatId
      }
      : registry.activeChatByProject,
    lastProject: activeProject === "" ? registry.lastProject : activeProject
  };
}

/** Forget one chat. When it was its project's active chat, that pointer goes
 *  with it so a later restore never looks for a chat that is not there. */
export function registryWithoutChat({ registry, project, chatId }: {
  registry: StoredChatRegistry;
  project: string;
  chatId: string;
}): StoredChatRegistry {
  const activeChatByProject = { ...registry.activeChatByProject };
  if (activeChatByProject[project] === chatId) {
    delete activeChatByProject[project];
  }

  return {
    ...registry,
    chats: registry.chats.filter(chat => !(chat.id === chatId && isSameProject(chat.project, project))),
    activeChatByProject
  };
}

/** One project's persisted chats, in the order they were last saved. */
export function chatsForProject({ registry, project }: {
  registry: StoredChatRegistry;
  project: string;
}): PersistedChat[] {
  return registry.chats.filter(chat => isSameProject(chat.project, project));
}

/** One persisted chat by id, when its project still has it. */
export function persistedChat({ registry, project, chatId }: {
  registry: StoredChatRegistry;
  project: string;
  chatId: string;
}) {
  return chatsForProject({
    registry,
    project
  }).find(chat => chat.id === chatId);
}

/** One chat row in the sidebar. A live row runs in this window, or - with
 *  `windowLabel` - in another PADE window, whose last reported `status` it
 *  shows. A recorded conversation no chat holds yet carries `resume` -
 *  picking it reopens that conversation. */
export interface SidebarChat {
  id: string;
  project: string;
  label: string;
  agentId: string;
  isLive: boolean;
  windowLabel?: string;
  status?: SessionStatus;
  resume?: RecordedConversation;
}

/** One project and its chats in the sidebar. */
export interface SidebarProject {
  project: string;
  isCurrent: boolean;
  chats: SidebarChat[];
}

/** A recorded conversation's row when the agent never titled it. */
const UNTITLED_CONVERSATION_LABEL = "Untitled conversation";

/** A session another window runs, as the sidebar needs it. */
type ElsewhereRow = Pick<ElsewhereSession, "id" | "project" | "name" | "agentId" | "conversationId" | "status" | "windowLabel">;

/** The project's recorded conversations that no row above already holds, as
 *  resumable rows. */
function recordedChats({ project, heldConversations, recorded }: {
  project: string;
  heldConversations: ReadonlySet<string | undefined>;
  recorded: readonly RecordedConversation[];
}): SidebarChat[] {
  return recorded
    .filter(conversation => !heldConversations.has(conversation.conversationId))
    .map(conversation => ({
      id: `${conversation.agentId}:${conversation.conversationId}`,
      project,
      label: conversation.title ?? UNTITLED_CONVERSATION_LABEL,
      agentId: conversation.agentId,
      isLive: false,
      resume: conversation
    }));
}

function chatRow({ chat, isLive }: {
  chat: PersistedChat;
  isLive: boolean;
}): SidebarChat {
  return {
    id: chat.id,
    project: chat.project,
    label: chat.label ?? chat.agent.label,
    agentId: chat.agent.id,
    isLive
  };
}

function elsewhereRow(session: ElsewhereRow): SidebarChat {
  return {
    id: session.id,
    project: session.project,
    label: session.name,
    agentId: session.agentId,
    isLive: true,
    windowLabel: session.windowLabel,
    status: session.status
  };
}

/** One project's rows: its live sessions here, then the ones other windows
 *  run, then its persisted chats nothing runs, then the recorded
 *  conversations none of those holds - so a running session never also shows
 *  as something to resume. */
function projectChats({ project, registry, liveSessions, liveLabels, elsewhereSessions, recorded }: {
  project: string;
  registry: StoredChatRegistry;
  liveSessions: readonly AgentSession[];
  liveLabels: Readonly<Record<string, string | undefined>>;
  elsewhereSessions: readonly ElsewhereRow[];
  recorded: readonly RecordedConversation[];
}) {
  const live = persistedChatsFromSessions({
    sessions: sessionsOfProject({
      sessions: liveSessions,
      project
    }),
    labels: liveLabels,
    now: 0
  });
  const liveIds = new Set(live.map(chat => chat.id));
  const elsewhere = sessionsOfProject({
    sessions: elsewhereSessions,
    project
  }).filter(session => !liveIds.has(session.id));
  const runningIds = new Set([...liveIds, ...elsewhere.map(session => session.id)]);
  const runningConversations = new Set([
    ...live.map(chat => chat.conversationId),
    ...elsewhere.map(session => session.conversationId)
  ]);
  const stored = chatsForProject({
    registry,
    project
  }).filter(chat => !runningIds.has(chat.id) && !runningConversations.has(chat.conversationId));
  return [
    ...live.map(chat => chatRow({
      chat,
      isLive: true
    })),
    ...elsewhere.map(elsewhereRow),
    ...stored.map(chat => chatRow({
      chat,
      isLive: false
    })),
    ...recordedChats({
      project,
      heldConversations: new Set([...runningConversations, ...stored.map(chat => chat.conversationId)]),
      recorded
    })
  ];
}

/** Build the sidebar's groups: the ordered projects, each with its chats.
 *  Every project's live sessions are live rows - this window's (their labels
 *  and order are the truth on screen) and the ones other PADE windows report
 *  - and below them its persisted chats and the conversations the agents
 *  recorded for it (their `/resume` lists). The project in front is the
 *  current one. A project with no chats still gets a group, so it can be
 *  switched to and started from. */
export function sidebarProjects({
  registry,
  projectOrder,
  activeProject,
  liveSessions,
  liveLabels,
  elsewhereSessions,
  recordedConversations
}: {
  registry: StoredChatRegistry;
  projectOrder: readonly string[];
  activeProject: string;
  liveSessions: readonly AgentSession[];
  liveLabels: Readonly<Record<string, string | undefined>>;
  elsewhereSessions: readonly ElsewhereRow[];
  recordedConversations: (project: string) => readonly RecordedConversation[];
}): SidebarProject[] {
  const seen = new Set<string>();
  const ordered: string[] = [];
  for (const project of projectOrder) {
    if (project === "" || seen.has(normalizePath(project))) {
      continue;
    }

    seen.add(normalizePath(project));
    ordered.push(project);
  }

  return ordered.map(project => ({
    project,
    isCurrent: activeProject !== "" && isSameProject(project, activeProject),
    chats: projectChats({
      project,
      registry,
      liveSessions,
      liveLabels,
      elsewhereSessions,
      recorded: recordedConversations(project)
    })
  }));
}
