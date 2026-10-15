// Owned-workspace lifecycle shared by the picker's Recent and Root sections:
// delete, move (→ permanent, still deletable), rename (→ promoted into the
// primary project root) with the inline-rename form state, and which temp
// workspace is being relabelled in PADE (its folder untouched). One instance lives
// in ProjectPicker and is handed to both sections and their row menus, so a
// rename started from either list drives the same form - and one delete
// confirmation dialog serves both.

import { errorMessage } from "@/lib/errors";
import { isOpenInAnotherWindow } from "@/lib/open-windows";
import { baseName } from "@/lib/paths";
import type { Settings, WindowInfo } from "@/lib/types";
import { nameError, parseInput, ProjectName } from "@/lib/validate";
import type { Relocator } from "@/lib/workspace-relocate";
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
  /** Delete the folder - via the app's relocator, so the sessions holding it as
   *  cwd are killed first and the removal isn't blocked by their lock. */
  ondelete: (path: string) => Promise<Settings>;
  /** Rename any project's folder where it stands, via the app's relocator. */
  onrenamefolder: Relocator["renameFolder"];
  /** The open PADE windows, so a project another one holds isn't moved. */
  openWindows: () => Promise<WindowInfo[]>;
  refresh: () => Promise<void>;
}

export type WorkspaceLifecycle = ReturnType<typeof createWorkspaceLifecycle>;

export function createWorkspaceLifecycle(host: LifecycleHost) {
  let renaming = $state<string | null>(null);
  let renameValue = $state("");
  let relabelTarget = $state<string | null>(null);
  let deleteTarget = $state<string | null>(null);
  let deleting = $state(false);
  let deleteError = $state<string | null>(null);
  let renameFolderTarget = $state<string | null>(null);
  let windowRows = $state<WindowInfo[]>([]);

  // A failed read keeps the last-known windows; the next change event retries.
  async function loadOpenWindows() {
    const [listing] = await Promise.allSettled([host.openWindows()]);
    if (listing.status === "fulfilled") {
      windowRows = listing.value;
    }
  }

  // Another window's agents hold its folder, and the picker can't stop them.
  function canRenameFolder(path: string) {
    return !isOpenInAnotherWindow({
      windowRows,
      path
    });
  }

  async function renameFolder(target: Parameters<Relocator["renameFolder"]>[0]) {
    const renamed = await host.onrenamefolder(target);
    await host.refresh();
    return renamed;
  }

  function startRename(path: string) {
    renaming = path;
    renameValue = baseName(path);
  }

  // Relabel (PADE-only name, folder untouched) is its own prompt - RelabelDialog,
  // rendered by the picker - so the lifecycle only tracks which row asked for it.
  function startRelabel(path: string) {
    relabelTarget = path;
  }

  function finishRelabel() {
    relabelTarget = null;
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
    const destination = await openDialog({
      directory: true,
      multiple: false
    });
    if (typeof destination !== "string") {
      return;
    }

    await host.onmove({
      from: path,
      destDir: destination
    });
    await host.refresh();
  }

  // Delete is confirmed in-app (ConfirmDialog, rendered by the picker) rather
  // than by an OS popup, so the three states of the flow live here: which path
  // is awaiting confirmation, whether its removal is in flight, and why it
  // failed - the folder can still be held open from outside PADE, and that
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
      // The bridge adopts settings first so the row leaves the Recent list (and
      // animates out) before the rescan catches the root lists up.
      await host.ondelete(path);
      deleteTarget = null;
      await host.refresh();
    } catch (error) {
      deleteTarget = path;
      deleteError = errorMessage({
        error,
        fallback: "Couldn’t delete that workspace"
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
    /** The temp workspace awaiting a PADE-only relabel (null = no prompt). */
    get relabelTarget() {
      return relabelTarget;
    },
    startRelabel,
    finishRelabel,
    /** The project awaiting a folder rename (null = no prompt). */
    get renameFolderTarget() {
      return renameFolderTarget;
    },
    startRenameFolder(path: string) {
      renameFolderTarget = path;
    },
    finishRenameFolder() {
      renameFolderTarget = null;
    },
    renameFolder,
    canRenameFolder,
    loadOpenWindows,
    get renameValue() {
      return renameValue;
    },
    set renameValue(value: string) {
      renameValue = value;
    },
    /** Live rename validation - the schema's message and the Save gate share
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
