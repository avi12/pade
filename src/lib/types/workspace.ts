import { z } from "zod";

import { Settings } from "@/lib/types/settings";

/** The manifest family that confirms a directory as a package (workspace
 *  member discovery - a folder is a package IFF it holds its own manifest). */
const Ecosystem = z.enum(["javascript", "rust", "go", "python"]);

/** One manifest-confirmed workspace member: its repo-relative `/`-joined
 *  directory (`""` = the root), the name its manifest declares (`null` when it
 *  declares none), and the manifest family that confirmed it (`null` only for
 *  a root without any manifest - kept so every file still has a bucket). */
export const WorkspaceMember = z.object({
  path: z.string(),
  name: z.string().nullable(),
  ecosystem: Ecosystem.nullable(),
  /** It holds its own `.git` - a checkout nested in the workspace, with a
   *  branch and a history of its own. Only these carry git chrome. */
  repository: z.boolean()
});

export type WorkspaceMember = z.infer<typeof WorkspaceMember>;

export const ProjectEntry = z.object({
  name: z.string(),
  path: z.string(),
  isGit: z.boolean()
});

export type ProjectEntry = z.infer<typeof ProjectEntry>;

/** Who names a workspace (`workspace_set_label`): the auto-namer, whose kebab-case
 *  suggestion only lands on an unlabelled workspace, or the user relabelling it in
 *  PADE, whose text is kept as typed and always wins. */
export const LabelSource = z.enum(["auto", "manual"]);

export type LabelSource = z.infer<typeof LabelSource>;

/** How `workspace_add_root` resolved. `added` carries the refreshed settings;
 *  `missing` (the path doesn't exist and creation wasn't requested) and
 *  `notADirectory` (the path names a file) are the two "didn't add" outcomes the
 *  picker acts on - prompting to create, or showing an inline error. */
export const AddRootStatus = z.enum(["added", "missing", "notADirectory"]);

export const AddRootOutcome = z.discriminatedUnion("status", [
  z.object({
    status: z.literal(AddRootStatus.enum.added),
    settings: Settings
  }),
  z.object({ status: z.literal(AddRootStatus.enum.missing) }),
  z.object({ status: z.literal(AddRootStatus.enum.notADirectory) })
]);

export type AddRootOutcome = z.infer<typeof AddRootOutcome>;

/** The result of saving a temp workspace by copy-migration: the new project dir
 *  the temp's files were copied into, and the dependency-install command to run
 *  there (`null` when there is nothing to reinstall). */
export const SaveMigration = z.object({
  path: z.string(),
  install: z.string().nullable()
});

export type SaveMigration = z.infer<typeof SaveMigration>;

/** A live probe of the path being typed into the add-root field. Instead of
 *  regex-guessing whether the text is a valid path, the backend just checks the
 *  filesystem: whether the path itself is a directory (`isDir`) or a file
 *  (`isFile`), and whether its parent exists (`parentExists`) - so a not-yet-made
 *  folder in a real place reads as "will create" and a stray string reads as
 *  invalid. `suggestions` are the child directories that complete what was typed,
 *  for the autocomplete; `completion` is the typed path completed as far as
 *  every matching folder agrees (empty when none match), for Ctrl+Space. */
export const PathProbe = z.object({
  isDir: z.boolean(),
  isFile: z.boolean(),
  parentExists: z.boolean(),
  suggestions: z.array(z.string()),
  completion: z.string()
});

export type PathProbe = z.infer<typeof PathProbe>;

/** The nothing-typed-yet probe: no disk knowledge, no completions. The shared
 *  reset every path field starts from, so an empty box never reads as a folder. */
export const EMPTY_PATH_PROBE: PathProbe = {
  isDir: false,
  isFile: false,
  parentExists: false,
  suggestions: [],
  completion: ""
};

/** A probe tagged with the exact text it describes - the field only trusts the
 *  disk flags once `path` matches the current (trimmed) input, since the probe is
 *  async + debounced. The shape `PathCombobox` binds back to its host. */
export interface TaggedPathProbe {
  path: string;
  result: PathProbe;
}

/** The steps a folder move goes through, in order (Rust `RelocateStep`). */
export const RelocateStep = z.enum(["renaming", "agentMemories", "editorRecents", "links", "settings"]);

export type RelocateStep = z.infer<typeof RelocateStep>;

/** Where a folder move is, keyed by the folder it started from. */
export const RelocateProgress = z.object({
  from: z.string(),
  step: RelocateStep
});

export type RelocateProgress = z.infer<typeof RelocateProgress>;
