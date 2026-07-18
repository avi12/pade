// Shared workspace-path helpers (DRY): one authoritative home for deriving a
// folder name from a path, reading a friendly display name from the labels map,
// and recognising a temporary workspace directory — reused by the app menu, the
// project picker, the shell, and the tasks panel so the "temp" logic never drifts.

/** The final path segment (folder name), or the whole path when it has none. */
export function baseName(path: string): string {
  return path.split(/[\\/]/).filter(Boolean).at(-1) ?? path;
}

/** A friendly display name: the assigned label if present, else the folder name. */
export function displayName(path: string, labels: Record<string, string>): string {
  return labels[path] ?? baseName(path);
}

/** The folder a path sits in, or null when it has no parent (a bare drive/root).
 *  Watching the parent — never the folder itself — is what lets the picker see a
 *  project appear or disappear without holding a handle on it. */
export function parentDir(path: string): string | null {
  const cut = path.replace(/[\\/]+$/, "").search(/[\\/][^\\/]*$/);
  return cut > 0 ? path.slice(0, cut) : null;
}

/** Whether a path is a PADE temporary workspace (…/workspaces/temp-<stamp>). */
export function isTemporaryWorkspace(path: string): boolean {
  return /[\\/]workspaces[\\/]temp-\d+$/.test(path);
}

/** Normalize a path for comparison. Separators and a trailing separator are
 *  cosmetic everywhere; casing is cosmetic only on Windows. A drive-letter path
 *  (`C:\…`) lives on case-insensitive NTFS, so it also folds to lower case —
 *  `C:\Repositories\` and `c:/repositories` compare equal. A POSIX path (a
 *  leading `/`), including WSL and its `/mnt/…` mounts, lives on a case-SENSITIVE
 *  filesystem, so it keeps its case: `/home/User/x` and `/home/user/x` stay
 *  distinct files. Used by the watcher, the workspace list, and the add-root
 *  dedup. */
export function normalizePath(path: string): string {
  const separated = path.replaceAll("\\", "/").replace(/\/+$/, "");
  const isWindowsDrivePath = /^[a-z]:/i.test(path);
  return isWindowsDrivePath ? separated.toLowerCase() : separated;
}
