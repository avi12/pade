// Pure bookkeeping for the left rail: which projects it lists and in what
// order, and how a search splits them into the Pinned and Recent sections.

import type { SidebarProject, StoredChatRegistry } from "@/lib/session/chats/chat-registry";
import { normalizePath } from "@/lib/util/paths";
import { matchesProjectFilter } from "@/lib/workspace/switcher/switcher-rows";

/** Every project the rail lists, pins first (in their saved order), then this
 *  window's project, the recents and any other project that still holds chats.
 *  Duplicates stay in; the grouping drops them, keeping each first place. */
export function sidebarProjectOrder({ current, pinned, recent, registry }: {
  current: string;
  pinned: readonly string[];
  recent: readonly string[];
  registry: StoredChatRegistry;
}) {
  const projectsWithChats = registry.chats.map(chat => chat.project);
  return [...pinned, current, ...recent, ...projectsWithChats];
}

/** The rail's two sections after the search: pinned projects, then every other
 *  one. A project matches on its shown name or its path; an empty search keeps
 *  them all. */
export function sidebarSections({ groups, pinned, query, labels }: {
  groups: readonly SidebarProject[];
  pinned: readonly string[];
  query: string;
  labels: Record<string, string>;
}) {
  const pinnedSet = new Set(pinned.map(normalizePath));
  const matching = groups.filter(group => matchesProjectFilter({
    project: group.project,
    query: query.trim(),
    labels
  }));
  return {
    pinned: matching.filter(group => pinnedSet.has(normalizePath(group.project))),
    recent: matching.filter(group => !pinnedSet.has(normalizePath(group.project)))
  };
}

/** The project Enter in the search jumps to - the first one the rail still
 *  shows, or none when the search matched nothing. */
export function firstListedProject(sections: ReturnType<typeof sidebarSections>) {
  return (sections.pinned[0] ?? sections.recent[0])?.project;
}
