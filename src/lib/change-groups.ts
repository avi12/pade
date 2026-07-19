// Pure grouping of Change Feed events into project buckets, so the feed reads as
// "what changed, per project" the way the mockup does. Ground truth first: when
// the backend's manifest census (`bridge.members`) confirms real workspace
// members, a change buckets under its deepest enclosing member — whole-segment
// longest-prefix, so `apps/web` never captures `apps/web-admin` — and anything
// outside every member falls into the repo itself. Only a workspace with no
// confirmed members falls back to the old folder-name convention (`apps/…`,
// `packages/…`, `services/…`); a directory's name proves nothing, so a
// root-level-package workspace (`frontend/` `backend/` `shared/` declared in
// `pnpm-workspace.yaml`) now splits per member instead of collapsing into one
// repo group — the gap the convention could never close.

import { baseName } from "@/lib/paths";
import type { ChangeEvent, WorkspaceMember } from "@/lib/types";

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

// The convention fallback: the project a changed path belongs to when no
// manifest members exist. A path under a container maps to its member folder
// (honouring an `@scope/name` two-segment member); everything else maps to the
// repo itself.
function projectOf({ path, workspaceRoot, repo }: {
  path: string;
  workspaceRoot: string;
  repo: Project;
}): Project {
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

// A member's badge role, read off its leading container segment when it uses a
// conventional one; any other member (a root-level `frontend/`) is badged App.
function memberRole(memberPath: string): GroupRole {
  const container = memberPath.split("/")[0]?.toLowerCase();
  if (container === undefined) {
    return GroupRole.App;
  }

  return CONTAINER_ROLES[container] ?? GroupRole.App;
}

// The deepest manifest-confirmed member enclosing a changed path. Whole
// segments are compared (case-insensitively, matching the root-prefix rule) so
// `apps/web` never captures `apps/web-admin`; outside every member the repo
// itself is the bucket.
function projectFromMembers({ path, workspaceRoot, members, repo }: {
  path: string;
  workspaceRoot: string;
  members: WorkspaceMember[];
  repo: Project;
}): Project {
  const segments = relativeSegments({
    path,
    workspaceRoot
  }).map(segment => segment.toLowerCase());

  let deepest: WorkspaceMember | undefined;
  let deepestDepth = 0;
  for (const member of members) {
    const memberSegments = member.path.toLowerCase().split("/");
    const encloses = memberSegments.length <= segments.length
      && memberSegments.every((segment, index) => segment === segments[index]);
    if (encloses && memberSegments.length > deepestDepth) {
      deepest = member;
      deepestDepth = memberSegments.length;
    }
  }
  if (deepest === undefined) {
    return repo;
  }

  return {
    id: deepest.path,
    name: deepest.name ?? baseName(deepest.path),
    role: memberRole(deepest.path)
  };
}

/** Bucket `events` (newest first) into project groups, summing line deltas.
 *  With manifest-confirmed `members` (see `bridge.members`) a change goes to
 *  its deepest enclosing member; without any, the folder-name convention is the
 *  fallback. The groups are ordered by their most-recent change, so the
 *  just-touched project leads — matching the flat feed's newest-first feel. */
export function groupChanges({ events, workspaceRoot, members = [] }: {
  events: ChangeEvent[];
  workspaceRoot: string;
  members?: WorkspaceMember[];
}): ChangeGroup[] {
  const repoName = baseName(workspaceRoot) || workspaceRoot;
  const repo: Project = {
    id: ".",
    name: repoName,
    role: GroupRole.App
  };
  // The root entry only signals "this workspace has a manifest"; grouping needs
  // the real (non-root) members, and with none the convention takes over.
  const manifestMembers = members.filter(member => member.path.length > 0);
  const groupsById = new Map<string, ChangeGroup>();

  for (const event of events) {
    const project = manifestMembers.length > 0
      ? projectFromMembers({
          path: event.path,
          workspaceRoot,
          members: manifestMembers,
          repo
        })
      : projectOf({
          path: event.path,
          workspaceRoot,
          repo
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

  return [...groupsById.values()].toSorted((first, second) => (second.events[0]?.ts ?? 0) - (first.events[0]?.ts ?? 0));
}
