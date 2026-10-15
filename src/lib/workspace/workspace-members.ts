// Reading a workspace member - the pure half of `stores/workspaceMembers`.
//
// The backend finds the members (src-tauri/members.rs: a package by its
// manifest, a checkout by its own `.git`) and reports each one as a path
// relative to the workspace root. Everything the UI needs on top of that - where
// the member actually lives on disk, what to call it, and which of them are
// repositories worth a branch chip - is decided here, so it is testable without
// a backend and has exactly one home.

import type { WorkspaceMember } from "@/lib/types";
import { baseName, childPath, isSegmentPrefix } from "@/lib/util/paths";

/** The Change Feed's id for the group that IS the workspace root (see
 *  `change-groups`), which as a member is the empty relative path. */
export const ROOT_GROUP_ID = ".";

/** A member-relative path's `/` segments; the root's empty path has none. */
function relativeSegments(path: string) {
  return path.length > 0 ? path.split("/") : [];
}

/** Where a member lives on disk. The root member's relative path is empty, so it
 *  is the root itself; anything else joins through the one separator home
 *  (`childPath`), with the backend's `/` segments converted on the way. */
export function memberPath({ root, member }: {
  root: string;
  member: WorkspaceMember;
}) {
  if (member.path.length === 0) {
    return root;
  }

  return childPath({
    parent: root,
    name: member.path.replaceAll("/", "\\")
  });
}

/** What to call a member: the name its manifest declares, else its own folder,
 *  and for the root - which often declares nothing - the workspace's folder. */
export function memberLabel({ root, member }: {
  root: string;
  member: WorkspaceMember;
}) {
  if (member.name) {
    return member.name;
  }

  const folder = member.path.length > 0 ? baseName(member.path) : baseName(root);
  return folder || root;
}

/** The member a Change Feed group belongs to, or undefined for a group the
 *  convention invented (the fallback grouping runs when no member was found, and
 *  those folder names are not members - they have no branch of their own). */
export function memberOfGroup({ groupId, members }: {
  groupId: string;
  members: WorkspaceMember[];
}) {
  const path = groupId === ROOT_GROUP_ID ? "" : groupId;
  return members.find(member => member.path === path);
}

/** The checkout a member's changes actually land in: itself when it holds its
 *  own `.git`, else the deepest checkout above it. A monorepo's packages are not
 *  repositories, so they report the branch of the repo that contains them -
 *  which is the branch their commits go to. */
export function enclosingRepository({ path, members }: {
  path: string;
  members: WorkspaceMember[];
}) {
  const segments = relativeSegments(path);
  let deepest: WorkspaceMember | undefined;
  for (const member of members) {
    if (!member.repository) {
      continue;
    }

    const isEnclosing = isSegmentPrefix({
      prefix: relativeSegments(member.path),
      segments
    });
    const isDeeper = deepest === undefined || member.path.length > deepest.path.length;
    if (isEnclosing && isDeeper) {
      deepest = member;
    }
  }

  return deepest;
}

/** The checkout whose branch a Change Feed group should show. A group the
 *  folder-name convention invented belongs to no member, so it reports the
 *  workspace's own checkout - the repo those files are in. */
export function repositoryOfGroup({ groupId, members }: {
  groupId: string;
  members: WorkspaceMember[];
}) {
  const member = memberOfGroup({
    groupId,
    members
  });
  return enclosingRepository({
    path: member?.path ?? "",
    members
  });
}

/** Whether the workspace is made of more than one part - the switcher only earns
 *  its place in the top bar when there is something to switch between. */
export function hasSeveralMembers(members: WorkspaceMember[]) {
  return members.length > 1;
}
