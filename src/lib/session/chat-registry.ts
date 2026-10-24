// The persisted chat registry: every project's chats, kept in localStorage
// so the left sidebar can list them, switch between them and bring them back
// after a relaunch. A chat is a session that has been closed with the app -
// its process is gone, but its agent, conversation id and label remain, so
// reopening it resumes that conversation (Claude through its pinned
// `--session-id`, the others through `agent_resume_args`).

import { Agent, ModelChoice } from "@/lib/types";
import type { AgentSession } from "@/lib/types";
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

function isSameProject(left: string, right: string) {
  return normalizePath(left) === normalizePath(right);
}

/** The live sessions of one project as persisted chats. Editor panes are
 *  skipped - they are not conversations. */
export function persistedChatsFromSessions({ project, sessions, labels, now }: {
  project: string;
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
      project,
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

/** Replace one project's chats with the ones it has now, leaving every other
 *  project's chats untouched. An empty replacement keeps the stored chats -
 *  a project being left passes through empty on its way out, and that must
 *  not erase what it had. */
export function registryWithProjectChats({ registry, project, chats, activeChatId }: {
  registry: StoredChatRegistry;
  project: string;
  chats: readonly PersistedChat[];
  activeChatId: string | null;
}): StoredChatRegistry {
  if (chats.length === 0) {
    return registry;
  }

  const others = registry.chats.filter(chat => !isSameProject(chat.project, project));
  return {
    chats: [...others, ...chats],
    activeChatByProject: activeChatId === null
      ? registry.activeChatByProject
      : {
        ...registry.activeChatByProject,
        [project]: activeChatId
      },
    lastProject: project
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

/** One chat row in the sidebar. */
export interface SidebarChat {
  id: string;
  project: string;
  label: string;
  agentId: string;
  isLive: boolean;
}

/** One project and its chats in the sidebar. */
export interface SidebarProject {
  project: string;
  isCurrent: boolean;
  chats: SidebarChat[];
}

/** Build the sidebar's groups: the ordered projects, each with its chats.
 *  The current project's chats come from its live sessions (their labels and
 *  order are the truth on screen); every other project's come from the
 *  registry. A project with no chats still gets a group, so it can be
 *  switched to and started from. */
export function sidebarProjects({ registry, projectOrder, currentProject, liveSessions, liveLabels }: {
  registry: StoredChatRegistry;
  projectOrder: readonly string[];
  currentProject: string;
  liveSessions: readonly AgentSession[];
  liveLabels: Readonly<Record<string, string | undefined>>;
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

  return ordered.map(project => {
    const isCurrent = currentProject !== "" && isSameProject(project, currentProject);
    if (isCurrent) {
      return {
        project,
        isCurrent,
        chats: persistedChatsFromSessions({
          project,
          sessions: liveSessions,
          labels: liveLabels,
          now: 0
        }).map(chat => ({
          id: chat.id,
          project,
          label: chat.label ?? chat.agent.label,
          agentId: chat.agent.id,
          isLive: true
        }))
      };
    }

    return {
      project,
      isCurrent,
      chats: chatsForProject({
        registry,
        project
      }).map(chat => ({
        id: chat.id,
        project,
        label: chat.label ?? chat.agent.label,
        agentId: chat.agent.id,
        isLive: false
      }))
    };
  });
}
