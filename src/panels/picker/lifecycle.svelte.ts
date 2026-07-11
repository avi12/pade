// Owned-workspace lifecycle shared by the picker's Recent and Root sections:
// delete, move (→ permanent, still deletable), and rename (→ promoted into the
// primary project root) with the inline-rename form state. One instance lives
// in ProjectPicker and is handed to both sections and their row menus, so a
// rename started from either list drives the same form.

import { workspace } from "@/lib/bridge";
import { baseName } from "@/lib/paths";
import type { Settings } from "@/lib/types";
import { nameError, parseInput, ProjectName } from "@/lib/validate";
import { ask, open as openDialog } from "@tauri-apps/plugin-dialog";

/** What the picker provides: the app-level move/rename flows (which handle the
 *  cwd locks), the settings sink, and the section re-scan. */
export interface LifecycleHost {
  isOwned: (path: string) => boolean;
  onmove: (target: {
    from: string;
    destDir: string;
  }) => Promise<string>;
  onrename: (target: {
    from: string;
    newName: string;
  }) => Promise<string>;
  applySettings: (settings: Settings) => void;
  refresh: () => Promise<void>;
}

export type WorkspaceLifecycle = ReturnType<typeof createWorkspaceLifecycle>;

export function createWorkspaceLifecycle(host: LifecycleHost) {
  let renaming = $state<string | null>(null);
  let renameValue = $state("");

  function startRename(path: string) {
    renaming = path;
    renameValue = baseName(path);
  }

  function cancelRename() {
    renaming = null;
  }

  async function commitRename(path: string) {
    const newName = parseInput({
      schema: ProjectName,
      raw: renameValue
    });
    if (!newName) {
      return;
    }

    await host.onrename({
      from: path,
      newName
    });
    renaming = null;
    await host.refresh();
  }

  async function moveWorkspace(path: string) {
    const dest = await openDialog({
      directory: true,
      multiple: false
    });
    if (typeof dest !== "string") {
      return;
    }

    await host.onmove({
      from: path,
      destDir: dest
    });
    await host.refresh();
  }

  async function deleteWorkspace(path: string) {
    const ok = await ask(`Delete this workspace and its files?\n\n${path}`, {
      title: "Delete workspace",
      kind: "warning"
    });
    if (!ok) {
      return;
    }

    host.applySettings(await workspace.delete(path));
  }

  return {
    /** The path whose row is showing the inline-rename form (null = none). */
    get renaming() {
      return renaming;
    },
    get renameValue() {
      return renameValue;
    },
    set renameValue(value: string) {
      renameValue = value;
    },
    /** Live rename validation — the schema's message and the Save gate share
     *  one check, so an invalid name can't reach a silent no-op rename. */
    get renameError() {
      return nameError(renameValue);
    },
    get renameValid() {
      return ProjectName.safeParse(renameValue).success;
    },
    isOwned: host.isOwned,
    startRename,
    cancelRename,
    commitRename,
    moveWorkspace,
    deleteWorkspace
  };
}
