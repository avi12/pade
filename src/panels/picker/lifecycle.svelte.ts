// Owned-workspace lifecycle shared by the picker's Recent and Root sections:
// delete, move (→ permanent, still deletable), and rename (→ promoted into the
// primary project root) with the inline-rename form state. One instance lives
// in ProjectPicker and is handed to both sections and their row menus, so a
// rename started from either list drives the same form — and one delete
// confirmation dialog serves both.

import { errorMessage } from "@/lib/errors";
import { baseName } from "@/lib/paths";
import type { Settings } from "@/lib/types";
import { nameError, parseInput, ProjectName } from "@/lib/validate";
import { open as openDialog } from "@tauri-apps/plugin-dialog";

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
  /** Delete the folder — via the app's relocator, so the sessions holding it as
   *  cwd are killed first and the removal isn't blocked by their lock. */
  ondelete: (path: string) => Promise<Settings>;
  applySettings: (settings: Settings) => void;
  refresh: () => Promise<void>;
}

export type WorkspaceLifecycle = ReturnType<typeof createWorkspaceLifecycle>;

export function createWorkspaceLifecycle(host: LifecycleHost) {
  let renaming = $state<string | null>(null);
  let renameValue = $state("");
  let deleteTarget = $state<string | null>(null);
  let deleting = $state(false);
  let deleteError = $state<string | null>(null);

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

  // Delete is confirmed in-app (ConfirmDialog, rendered by the picker) rather
  // than by an OS popup, so the three states of the flow live here: which path
  // is awaiting confirmation, whether its removal is in flight, and why it
  // failed — the folder can still be held open from outside PADE, and that
  // reason belongs in front of the user instead of a swallowed rejection.
  function requestDelete(path: string) {
    deleteTarget = path;
    deleteError = null;
  }

  function cancelDelete() {
    if (deleting) {
      return;
    }

    deleteTarget = null;
  }

  // The removal itself, shared by the confirmed and the shift-click paths. A
  // failure re-opens (or keeps) the dialog carrying the reason, so even a skipped
  // confirmation can't fail silently.
  async function removeWorkspace(path: string) {
    deleting = true;
    deleteError = null;
    try {
      // Settings land first so the row leaves the Recent list (and animates out)
      // the moment the folder is gone; the rescan then catches the root lists up.
      host.applySettings(await host.ondelete(path));
      deleteTarget = null;
      await host.refresh();
    } catch (error) {
      deleteTarget = path;
      deleteError = errorMessage({
        error,
        fallback: "Couldn’t delete that workspace."
      });
    } finally {
      deleting = false;
    }
  }

  async function confirmDelete() {
    if (!deleteTarget || deleting) {
      return;
    }

    await removeWorkspace(deleteTarget);
  }

  /** Shift-click on Delete: skip the confirmation and remove it straight away. */
  async function deleteNow(path: string) {
    if (deleting) {
      return;
    }

    await removeWorkspace(path);
  }

  return {
    /** The path awaiting delete confirmation (null = no dialog). */
    get deleteTarget() {
      return deleteTarget;
    },
    get deleting() {
      return deleting;
    },
    get deleteError() {
      return deleteError;
    },
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
    requestDelete,
    cancelDelete,
    confirmDelete,
    deleteNow
  };
}
