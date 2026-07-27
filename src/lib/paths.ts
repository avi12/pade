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

/** The last two path segments joined as "parent/leaf" (or the leaf alone when the
 *  path has only one) — the compact, legible directory label that fits the top bar
 *  without eating it. One authoritative home for the last-two-segments split —
 *  module-private, reached through `shortDisplayName` so the label override always
 *  applies. */
function shortDirectory(path: string): string {
  return path.split(/[\\/]/).filter(Boolean).slice(-2).join("/");
}

/** A friendly two-segment display name: the assigned label if present, else the
 *  compact "parent/leaf" directory (see `shortDirectory`). The top-bar project chip and
 *  the switcher's open-window rows both read it so they never drift apart. */
export function shortDisplayName(path: string, labels: Record<string, string>): string {
  return labels[path] ?? shortDirectory(path);
}

/** The folder a path sits in, or null when it has no parent (a bare drive/root).
 *  Watching the parent — never the folder itself — is what lets the picker see a
 *  project appear or disappear without holding a handle on it. */
export function parentDirectory(path: string): string | null {
  const cut = path.replace(/[\\/]+$/, "").search(/[\\/][^\\/]*$/);
  return cut > 0 ? path.slice(0, cut) : null;
}

/** Compose the path of a named child under a parent directory — the one home for
 *  the platform separator so the "<root>\<name>" join never drifts between the
 *  project picker's new-project probe and the app menu's Save-this-workspace
 *  collision check. */
export function childPath({ parent, name }: {
  parent: string;
  name: string;
}): string {
  return `${parent}\\${name}`;
}

/** Whether a path is a PADE temporary workspace (…/workspaces/temp-<stamp>). */
export function isTemporaryWorkspace(path: string): boolean {
  return /[\\/]workspaces[\\/]temp-\d+$/.test(path);
}

/** A path shown relative to a workspace root, root-anchored with a leading "/":
 *  "/backend/convex" for a nested path, "/" for the root itself. A path outside
 *  the root (a worktree sibling, a moved file) falls back to the absolute form —
 *  a wrong but honest label beats a fabricated relative one. Comparison rides
 *  `normalizePath`, so separators, trailing slashes, and Windows casing don't
 *  break the match; the returned segments use "/" uniformly. */
export function relativeToRoot({ path, root }: {
  path: string;
  root: string;
}): string {
  const normalizedPath = normalizePath(path);
  const normalizedRoot = normalizePath(root);
  if (normalizedPath === normalizedRoot) {
    return "/";
  }

  if (!normalizedPath.startsWith(`${normalizedRoot}/`)) {
    return path;
  }

  // Slice the ORIGINAL path so a case-sensitive tail keeps its casing; only
  // the length of the normalized root matters for the cut.
  const tail = path
    .replaceAll("\\", "/")
    .slice(normalizedRoot.length + 1);
  return `/${tail}`;
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
