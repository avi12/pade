import { z } from "zod";

import { NO_REPLY } from "@/lib/ipc/domains/no-reply";
import type { PathData } from "@/lib/ipc/domains/path-data";
import { Settings } from "@/lib/types/settings";
import {
  AddRootOutcome,
  PathProbe,
  ProjectEntry,
  RelocateProgress,
  SaveMigration,
  WorkspaceMember
} from "@/lib/types/workspace";
import type { LabelSource } from "@/lib/types/workspace";

/** A workspace moved under a new folder name, optionally into another saved
 *  root (omitted → the primary root). */
interface WorkspaceRename {
  from: string;
  newName: string;
  root?: string;
}

export const WORKSPACE_COMMANDS = {
  membersList: "members_list",
  workspaceAddRoot: "workspace_add_root",
  workspaceRemoveRoot: "workspace_remove_root",
  workspaceProbePath: "workspace_probe_path",
  workspaceScan: "workspace_scan",
  workspaceOpen: "workspace_open",
  handoffDocDelete: "handoff_doc_delete",
  workspaceTemp: "workspace_temp",
  workspaceMove: "workspace_move",
  workspaceRename: "workspace_rename",
  workspaceRenameFolder: "workspace_rename_folder",
  workspaceSaveMigrate: "workspace_save_migrate",
  workspaceSetLabel: "workspace_set_label",
  workspaceDelete: "workspace_delete",
  workspacePrune: "workspace_prune",
  workspaceCreate: "workspace_create",
  workspaceClearRecent: "workspace_clear_recent",
  workspaceSetPinned: "workspace_set_pinned",
  workspaceRemoveRecent: "workspace_remove_recent",
  workspaceSetPinnedOrder: "workspace_set_pinned_order",
  workspaceDeleteDirectory: "workspace_delete_directory"
} as const;

export const WORKSPACE_EVENTS = {
  relocateProgress: "relocate://progress"
} as const;

export interface WorkspaceCommandProtocol {
  /** Confirmed members of the workspace at `root`; the root itself is first. */
  [WORKSPACE_COMMANDS.membersList]: (data: { root: string }) => WorkspaceMember[];
  /** Add a root folder. `create` makes a missing path first; the outcome says
   *  whether it was added, is missing, or names a file. */
  [WORKSPACE_COMMANDS.workspaceAddRoot]: (data: { path: string; create: boolean }) => AddRootOutcome;
  [WORKSPACE_COMMANDS.workspaceRemoveRoot]: (data: PathData) => Settings;
  /** Whether a partially-typed path exists (as a dir or a file), plus
   *  child-directory completions for the add-root autocomplete. */
  [WORKSPACE_COMMANDS.workspaceProbePath]: (data: PathData) => PathProbe;
  [WORKSPACE_COMMANDS.workspaceScan]: (data: { root: string }) => ProjectEntry[];
  [WORKSPACE_COMMANDS.workspaceOpen]: (data: PathData) => void;
  /** Delete a consumed auto-handoff doc (`continue-*.md` in the project dir -
   *  the backend refuses anything else). */
  [WORKSPACE_COMMANDS.handoffDocDelete]: (data: { dir: string; name: string }) => void;
  /** Create a throwaway workspace, mark it owned, and open it; answers its path. */
  [WORKSPACE_COMMANDS.workspaceTemp]: () => string;
  [WORKSPACE_COMMANDS.workspaceMove]: (data: { from: string; destDir: string }) => string;
  /** Promote a temp workspace into a saved root under a new name. */
  [WORKSPACE_COMMANDS.workspaceRename]: (data: WorkspaceRename) => string;
  /** Rename any workspace's folder where it stands, carrying every agent's
   *  memory of it and PADE's settings along; answers the new path. */
  [WORKSPACE_COMMANDS.workspaceRenameFolder]: (data: Omit<WorkspaceRename, "root">) => string;
  /** Save a temp workspace by copying its files into a fresh saved project;
   *  answers the new dir plus the dependency-install command to run there. */
  [WORKSPACE_COMMANDS.workspaceSaveMigrate]: (data: WorkspaceRename) => SaveMigration;
  /** Set a friendly display label (no disk rename). An `auto` name never
   *  replaces an existing label; a `manual` one always does. */
  [WORKSPACE_COMMANDS.workspaceSetLabel]: (data: { path: string; name: string; source: LabelSource }) => Settings;
  [WORKSPACE_COMMANDS.workspaceDelete]: (data: PathData) => Settings;
  /** Settings with every vanished folder forgotten. */
  [WORKSPACE_COMMANDS.workspacePrune]: () => Settings;
  [WORKSPACE_COMMANDS.workspaceCreate]: (data: { root: string; name: string }) => string;
  [WORKSPACE_COMMANDS.workspaceClearRecent]: () => Settings;
  [WORKSPACE_COMMANDS.workspaceSetPinned]: (data: { path: string; pinned: boolean }) => Settings;
  /** Forget a project from the switcher (recents + pins); folder untouched. */
  [WORKSPACE_COMMANDS.workspaceRemoveRecent]: (data: PathData) => Settings;
  /** Persist a drag-reordered pin order (reorders existing pins only). */
  [WORKSPACE_COMMANDS.workspaceSetPinnedOrder]: (data: { paths: string[] }) => Settings;
  /** Delete ANY project directory from disk and forget it. The caller raises a
   *  confirmation and releases the folder first. */
  [WORKSPACE_COMMANDS.workspaceDeleteDirectory]: (data: PathData) => Settings;
}

export const WORKSPACE_RESPONSE_SCHEMAS = {
  [WORKSPACE_COMMANDS.membersList]: z.array(WorkspaceMember),
  [WORKSPACE_COMMANDS.workspaceAddRoot]: AddRootOutcome,
  [WORKSPACE_COMMANDS.workspaceRemoveRoot]: Settings,
  [WORKSPACE_COMMANDS.workspaceProbePath]: PathProbe,
  [WORKSPACE_COMMANDS.workspaceScan]: z.array(ProjectEntry),
  [WORKSPACE_COMMANDS.workspaceOpen]: NO_REPLY,
  [WORKSPACE_COMMANDS.handoffDocDelete]: NO_REPLY,
  [WORKSPACE_COMMANDS.workspaceTemp]: z.string(),
  [WORKSPACE_COMMANDS.workspaceMove]: z.string(),
  [WORKSPACE_COMMANDS.workspaceRename]: z.string(),
  [WORKSPACE_COMMANDS.workspaceRenameFolder]: z.string(),
  [WORKSPACE_COMMANDS.workspaceSaveMigrate]: SaveMigration,
  [WORKSPACE_COMMANDS.workspaceSetLabel]: Settings,
  [WORKSPACE_COMMANDS.workspaceDelete]: Settings,
  [WORKSPACE_COMMANDS.workspacePrune]: Settings,
  [WORKSPACE_COMMANDS.workspaceCreate]: z.string(),
  [WORKSPACE_COMMANDS.workspaceClearRecent]: Settings,
  [WORKSPACE_COMMANDS.workspaceSetPinned]: Settings,
  [WORKSPACE_COMMANDS.workspaceRemoveRecent]: Settings,
  [WORKSPACE_COMMANDS.workspaceSetPinnedOrder]: Settings,
  [WORKSPACE_COMMANDS.workspaceDeleteDirectory]: Settings
} satisfies { [Key in keyof WorkspaceCommandProtocol]: z.ZodType<ReturnType<WorkspaceCommandProtocol[Key]>> };

export interface WorkspaceEventProtocol {
  /** A folder move reached its next step - shown by the prompt waiting on it. */
  [WORKSPACE_EVENTS.relocateProgress]: (data: RelocateProgress) => void;
}

export const WORKSPACE_PAYLOAD_SCHEMAS = {
  [WORKSPACE_EVENTS.relocateProgress]: RelocateProgress
} satisfies { [Key in keyof WorkspaceEventProtocol]: z.ZodType<Parameters<WorkspaceEventProtocol[Key]>[0]> };
