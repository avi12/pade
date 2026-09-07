// The one frontend home for "what is this workspace made of?" (DRY/SSOT).
//
// The backend finds the parts (src-tauri/members.rs: a package by its manifest,
// a checkout by its own `.git`); this store owns the single fetch per project,
// the branch each checkout is on, and which member the workspace is currently
// pointed at. Every surface that shows or acts on a member reads it from here —
// the top bar's switcher and branch pill, the Change Feed's grouping and its
// per-group branch, and the working directory a new agent or an editor opens in
// — so a workspace can never be two different shapes in two places at once.
//
// The pure half (where a member lives, what to call it) is `lib/workspace-members`.

import { members, vcs } from "@/lib/bridge";
import type { WorkspaceMember } from "@/lib/types";
import { memberPath } from "@/lib/workspace-members";

let root = $state("");
let list = $state<WorkspaceMember[]>([]);
/** HEAD branch per member, keyed by the member's absolute path. Only checkouts
 *  appear: `vcs_branch_of` omits a path that isn't a repo or is detached. */
let branchByPath = $state<Record<string, string>>({});
/** The member every "where does this start?" decision points at — always a real
 *  member path, defaulting to the workspace root. */
let active = $state("");

/** The workspace's members, root first. Empty until a project is loaded. */
export function workspaceMembers(): WorkspaceMember[] {
  return list;
}

/** Where a member lives on disk. */
export function pathOfMember(member: WorkspaceMember): string {
  return memberPath({
    root,
    member
  });
}

/** The branch a member is on, or undefined when it isn't a checkout (or is on a
 *  detached HEAD). */
export function branchOfMember(path: string): string | undefined {
  return branchByPath[path];
}

/** The member the workspace is pointed at — the directory a new agent tab, a
 *  worktree launch or the editor opens in. Falls back to the workspace root. */
export function activeMemberPath(): string {
  return active || root;
}

/** Point the workspace at one of its members. */
export function selectMember(path: string): void {
  active = path;
}

/** Load the members of `projectRoot`, then their branches. A project switch
 *  resets the selection to the root: the member you had chosen belongs to the
 *  workspace you just left. */
export async function loadWorkspaceMembers(projectRoot: string): Promise<void> {
  root = projectRoot;
  active = projectRoot;
  list = [];
  branchByPath = {};

  if (!projectRoot) {
    return;
  }

  const found = await members.list(projectRoot).catch((): WorkspaceMember[] => []);
  if (root !== projectRoot) {
    return;
  }

  list = found;
  await refreshMemberBranches();
}

/** Re-read every checkout's HEAD — on load, and again whenever git state moves
 *  (a branch switch, a `git init`, a fresh clone inside the workspace). */
export async function refreshMemberBranches(): Promise<void> {
  const asked = root;
  const paths = list
    .filter(member => member.repository)
    .map(member => pathOfMember(member));
  if (paths.length === 0) {
    branchByPath = {};
    return;
  }

  const heads = await vcs.branchOf(paths).catch((): Record<string, string> => ({}));
  if (root !== asked) {
    return;
  }

  branchByPath = heads;
}
