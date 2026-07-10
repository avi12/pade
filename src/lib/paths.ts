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

/** Whether a path is a PADE temporary workspace (…/workspaces/temp-<stamp>). */
export function isTemporaryWorkspace(path: string): boolean {
  return /[\\/]workspaces[\\/]temp-\d+$/.test(path);
}
