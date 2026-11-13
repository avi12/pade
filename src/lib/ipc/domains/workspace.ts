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
  MEMBERS_LIST: "members_list",
  WORKSPACE_ADD_ROOT: "workspace_add_root",
  WORKSPACE_REMOVE_ROOT: "workspace_remove_root",
  WORKSPACE_PROBE_PATH: "workspace_probe_path",
  WORKSPACE_SCAN: "workspace_scan",
  WORKSPACE_OPEN: "workspace_open",
  HANDOFF_DOC_DELETE: "handoff_doc_delete",
  WORKSPACE_TEMP: "workspace_temp",
  WORKSPACE_MOVE: "workspace_move",
  WORKSPACE_RENAME: "workspace_rename",
  WORKSPACE_RENAME_FOLDER: "workspace_rename_folder",
  WORKSPACE_SAVE_MIGRATE: "workspace_save_migrate",
  WORKSPACE_SET_LABEL: "workspace_set_label",
  WORKSPACE_DELETE: "workspace_delete",
  WORKSPACE_PRUNE: "workspace_prune",
  WORKSPACE_CREATE: "workspace_create",
  WORKSPACE_CLEAR_RECENT: "workspace_clear_recent",
  WORKSPACE_SET_PINNED: "workspace_set_pinned",
  WORKSPACE_REMOVE_RECENT: "workspace_remove_recent",
  WORKSPACE_SET_PINNED_ORDER: "workspace_set_pinned_order",
  WORKSPACE_DELETE_DIRECTORY: "workspace_delete_directory"
} as const;

export const WORKSPACE_EVENTS = {
  RELOCATE_PROGRESS: "relocate://progress"
} as const;

export interface WorkspaceCommandProtocol {
  /** Confirmed members of the workspace at `root`; the root itself is first. */
  [WORKSPACE_COMMANDS.MEMBERS_LIST]: (data: { root: string }) => WorkspaceMember[];
  /** Add a root folder. `create` makes a missing path first; the outcome says
   *  whether it was added, is missing, or names a file. */
  [WORKSPACE_COMMANDS.WORKSPACE_ADD_ROOT]: (data: { path: string; create: boolean }) => AddRootOutcome;
  [WORKSPACE_COMMANDS.WORKSPACE_REMOVE_ROOT]: (data: PathData) => Settings;
  /** Whether a partially-typed path exists (as a dir or a file), plus
   *  child-directory completions for the add-root autocomplete. */
  [WORKSPACE_COMMANDS.WORKSPACE_PROBE_PATH]: (data: PathData) => PathProbe;
  [WORKSPACE_COMMANDS.WORKSPACE_SCAN]: (data: { root: string }) => ProjectEntry[];
  [WORKSPACE_COMMANDS.WORKSPACE_OPEN]: (data: PathData) => void;
  /** Delete a consumed auto-handoff doc (`continue-*.md` in the project dir -
   *  the backend refuses anything else). */
  [WORKSPACE_COMMANDS.HANDOFF_DOC_DELETE]: (data: { dir: string; name: string }) => void;
  /** Create a throwaway workspace, mark it owned, and open it; answers its path. */
  [WORKSPACE_COMMANDS.WORKSPACE_TEMP]: () => string;
  [WORKSPACE_COMMANDS.WORKSPACE_MOVE]: (data: { from: string; destDir: string }) => string;
  /** Promote a temp workspace into a saved root under a new name. */
  [WORKSPACE_COMMANDS.WORKSPACE_RENAME]: (data: WorkspaceRename) => string;
  /** Rename any workspace's folder where it stands, carrying every agent's
   *  memory of it and PADE's settings along; answers the new path. */
  [WORKSPACE_COMMANDS.WORKSPACE_RENAME_FOLDER]: (data: Omit<WorkspaceRename, "root">) => string;
  /** Save a temp workspace by copying its files into a fresh saved project;
   *  answers the new dir plus the dependency-install command to run there. */
  [WORKSPACE_COMMANDS.WORKSPACE_SAVE_MIGRATE]: (data: WorkspaceRename) => SaveMigration;
  /** Set a friendly display label (no disk rename). An `auto` name never
   *  replaces an existing label; a `manual` one always does. */
  [WORKSPACE_COMMANDS.WORKSPACE_SET_LABEL]: (data: { path: string; name: string; source: LabelSource }) => Settings;
  [WORKSPACE_COMMANDS.WORKSPACE_DELETE]: (data: PathData) => Settings;
  /** Settings with every vanished folder forgotten. */
  [WORKSPACE_COMMANDS.WORKSPACE_PRUNE]: () => Settings;
  [WORKSPACE_COMMANDS.WORKSPACE_CREATE]: (data: { root: string; name: string }) => string;
  [WORKSPACE_COMMANDS.WORKSPACE_CLEAR_RECENT]: () => Settings;
  [WORKSPACE_COMMANDS.WORKSPACE_SET_PINNED]: (data: { path: string; pinned: boolean }) => Settings;
  /** Forget a project from the switcher (recents + pins); folder untouched. */
  [WORKSPACE_COMMANDS.WORKSPACE_REMOVE_RECENT]: (data: PathData) => Settings;
  /** Persist a drag-reordered pin order (reorders existing pins only). */
  [WORKSPACE_COMMANDS.WORKSPACE_SET_PINNED_ORDER]: (data: { paths: string[] }) => Settings;
  /** Delete ANY project directory from disk and forget it. The caller raises a
   *  confirmation and releases the folder first. */
  [WORKSPACE_COMMANDS.WORKSPACE_DELETE_DIRECTORY]: (data: PathData) => Settings;
}

export const WORKSPACE_RESPONSE_SCHEMAS = {
  [WORKSPACE_COMMANDS.MEMBERS_LIST]: z.array(WorkspaceMember),
  [WORKSPACE_COMMANDS.WORKSPACE_ADD_ROOT]: AddRootOutcome,
  [WORKSPACE_COMMANDS.WORKSPACE_REMOVE_ROOT]: Settings,
  [WORKSPACE_COMMANDS.WORKSPACE_PROBE_PATH]: PathProbe,
  [WORKSPACE_COMMANDS.WORKSPACE_SCAN]: z.array(ProjectEntry),
  [WORKSPACE_COMMANDS.WORKSPACE_OPEN]: NO_REPLY,
  [WORKSPACE_COMMANDS.HANDOFF_DOC_DELETE]: NO_REPLY,
  [WORKSPACE_COMMANDS.WORKSPACE_TEMP]: z.string(),
  [WORKSPACE_COMMANDS.WORKSPACE_MOVE]: z.string(),
  [WORKSPACE_COMMANDS.WORKSPACE_RENAME]: z.string(),
  [WORKSPACE_COMMANDS.WORKSPACE_RENAME_FOLDER]: z.string(),
  [WORKSPACE_COMMANDS.WORKSPACE_SAVE_MIGRATE]: SaveMigration,
  [WORKSPACE_COMMANDS.WORKSPACE_SET_LABEL]: Settings,
  [WORKSPACE_COMMANDS.WORKSPACE_DELETE]: Settings,
  [WORKSPACE_COMMANDS.WORKSPACE_PRUNE]: Settings,
  [WORKSPACE_COMMANDS.WORKSPACE_CREATE]: z.string(),
  [WORKSPACE_COMMANDS.WORKSPACE_CLEAR_RECENT]: Settings,
  [WORKSPACE_COMMANDS.WORKSPACE_SET_PINNED]: Settings,
  [WORKSPACE_COMMANDS.WORKSPACE_REMOVE_RECENT]: Settings,
  [WORKSPACE_COMMANDS.WORKSPACE_SET_PINNED_ORDER]: Settings,
  [WORKSPACE_COMMANDS.WORKSPACE_DELETE_DIRECTORY]: Settings
} satisfies { [Key in keyof WorkspaceCommandProtocol]: z.ZodType<ReturnType<WorkspaceCommandProtocol[Key]>> };

export interface WorkspaceEventProtocol {
  /** A folder move reached its next step - shown by the prompt waiting on it. */
  [WORKSPACE_EVENTS.RELOCATE_PROGRESS]: (data: RelocateProgress) => void;
}

export const WORKSPACE_PAYLOAD_SCHEMAS = {
  [WORKSPACE_EVENTS.RELOCATE_PROGRESS]: RelocateProgress
} satisfies { [Key in keyof WorkspaceEventProtocol]: z.ZodType<Parameters<WorkspaceEventProtocol[Key]>[0]> };
