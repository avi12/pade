// Pure bookkeeping for the left rail: which projects it lists and in what
// order, and how a search splits them into the Pinned and Recent sections.

import type { SidebarProject } from "@/lib/session/chats/chat-registry";
import { normalizePath } from "@/lib/util/paths";
import { matchesProjectFilter } from "@/lib/workspace/switcher/switcher-rows";

/** Every project the rail lists, pins first (in their saved order), then this
 *  window's project and the recents. Saved chats never list a project on their
 *  own: one that was deleted, removed from the list or cleaned up would stay
 *  in the rail with no folder behind it, where it could be neither removed nor
 *  deleted. Duplicates stay in; the grouping drops them, keeping each first
 *  place. */
export function sidebarProjectOrder({ current, pinned, recent }: {
  current: string;
  pinned: readonly string[];
  recent: readonly string[];
}) {
  return [...pinned, current, ...recent];
}

// A project with a session running - here or in another window - or the one in
// front, which may still be launching its first.
function isActiveProject(group: SidebarProject) {
  return group.isCurrent || group.chats.some(chat => chat.isLive);
}

/** The rail's two sections after the search: pinned projects, then every other
 *  one - the active ones first, each part keeping its recent order, so a
 *  project whose last session stops sinks below the ones still running. A
 *  project matches on its shown name or its path; an empty search keeps them
 *  all. */
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
  const unpinned = matching.filter(group => !pinnedSet.has(normalizePath(group.project)));
  return {
    pinned: matching.filter(group => pinnedSet.has(normalizePath(group.project))),
    recent: [...unpinned.filter(isActiveProject), ...unpinned.filter(group => !isActiveProject(group))]
  };
}

/** The project Enter in the search jumps to - the first one the rail still
 *  shows, or none when the search matched nothing. */
export function firstListedProject(sections: ReturnType<typeof sidebarSections>) {
  return (sections.pinned[0] ?? sections.recent[0])?.project;
}
