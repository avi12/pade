// One home for turning a thrown value into something a user can read. Tauri
// rejects an IPC call with the Rust `Err(String)`, but a bug on the way there
// can throw anything — so every catch funnels through here instead of each
// caller re-deriving the same `typeof error === "string"` check.

export function errorMessage({ error, fallback }: {
  error: unknown;
  fallback: string;
}): string {
  if (typeof error === "string" && error.trim()) {
    return error;
  }

  return error instanceof Error && error.message ? error.message : fallback;
}
