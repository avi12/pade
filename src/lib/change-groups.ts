// Pure grouping of Change Feed events into project buckets, so the feed reads as
// "what changed, per project" the way the mockup does. Monorepo-aware by
// convention: a change under a known workspace container (`apps/…`,
// `packages/…`, `services/…`) is bucketed by its member folder; anything else
// falls into the repo itself. A single-project repo therefore yields one group,
// a multi-project monorepo yields one per touched member — no backend needed.

import { baseName } from "@/lib/paths";
import type { ChangeEvent } from "@/lib/types";

/** Coarse project role driving the group's badge. One authoritative home. */
export const GroupRole = {
  App: "app",
  Lib: "lib",
  Service: "service"
} as const;
export type GroupRole = (typeof GroupRole)[keyof typeof GroupRole];

export interface ChangeGroup {
  /** Stable key (the container-relative project path, or `.` for the repo). */
  id: string;
  /** Display name — the member folder, or the repo's own folder name. */
  name: string;
  role: GroupRole;
  /** This group's events, newest first (the input order is preserved). */
  events: ChangeEvent[];
  /** Summed line deltas across the group's events. */
  added: number;
  removed: number;
}

// Monorepo container folder → the role its members carry. The authoritative list
// of container conventions; a first path segment matching one (case-insensitively)
// means the segment after it is a project member.
const CONTAINER_ROLES: Record<string, GroupRole> = {
  apps: GroupRole.App,
  app: GroupRole.App,
  packages: GroupRole.Lib,
  libs: GroupRole.Lib,
  lib: GroupRole.Lib,
  crates: GroupRole.Lib,
  modules: GroupRole.Lib,
  plugins: GroupRole.Lib,
  services: GroupRole.Service,
  service: GroupRole.Service,
  servers: GroupRole.Service,
  server: GroupRole.Service
};

interface Project {
  id: string;
  name: string;
  role: GroupRole;
}

// A path's segments relative to the workspace root, separators normalised and
// case preserved (the display name keeps its real casing). Matching the root
// prefix is case-insensitive so a drive-letter difference on Windows still lines
// up; the events genuinely live under the root, so this can't mis-attribute.
function relativeSegments({ path, workspaceRoot }: {
  path: string;
  workspaceRoot: string;
}): string[] {
  const forwardPath = path.replaceAll("\\", "/");
  const forwardRoot = workspaceRoot.replaceAll("\\", "/").replace(/\/+$/, "");
  const isUnderRoot = forwardPath.toLowerCase().startsWith(`${forwardRoot.toLowerCase()}/`);
  const relative = isUnderRoot ? forwardPath.slice(forwardRoot.length + 1) : forwardPath;
  return relative.split("/").filter(segment => segment.length > 0);
}

// The project a changed path belongs to. A path under a container maps to its
// member folder (honouring an `@scope/name` two-segment member); everything else
// maps to the repo itself.
function projectOf({ path, workspaceRoot, repoName }: {
  path: string;
  workspaceRoot: string;
  repoName: string;
}): Project {
  const repo: Project = {
    id: ".",
    name: repoName,
    role: GroupRole.App
  };

  const segments = relativeSegments({
    path,
    workspaceRoot
  });
  const container = segments[0]?.toLowerCase();
  const role = container === undefined ? undefined : CONTAINER_ROLES[container];
  if (role === undefined || segments.length < 2) {
    return repo;
  }

  const isScopedMember = segments[1].startsWith("@") && segments.length >= 3;
  const member = isScopedMember ? `${segments[1]}/${segments[2]}` : segments[1];
  return {
    id: `${container}/${member}`,
    name: member,
    role
  };
}

/** Bucket `events` (newest first) into project groups, summing line deltas. The
 *  groups are ordered by their most-recent change, so the just-touched project
 *  leads — matching the flat feed's newest-first feel. */
export function groupChanges({ events, workspaceRoot }: {
  events: ChangeEvent[];
  workspaceRoot: string;
}): ChangeGroup[] {
  const repoName = baseName(workspaceRoot) || workspaceRoot;
  const groupsById = new Map<string, ChangeGroup>();

  for (const event of events) {
    const project = projectOf({
      path: event.path,
      workspaceRoot,
      repoName
    });
    let group = groupsById.get(project.id);
    if (group === undefined) {
      group = {
        id: project.id,
        name: project.name,
        role: project.role,
        events: [],
        added: 0,
        removed: 0
      };
      groupsById.set(project.id, group);
    }

    group.events.push(event);
    group.added += event.added;
    group.removed += event.removed;
  }

  const groups = [...groupsById.values()];
  groups.sort((a, b) => (b.events[0]?.ts ?? 0) - (a.events[0]?.ts ?? 0));
  return groups;
}
