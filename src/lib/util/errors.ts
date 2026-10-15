// One home for turning a thrown value into something a user can read. Tauri
// rejects an IPC call with the Rust `Err(String)`, but a bug on the way there
// can throw anything - so every catch funnels through here instead of each
// caller re-deriving the same `typeof error === "string"` check.

import { z } from "zod";

/** A thrown string with something in it, kept as thrown (untrimmed). */
const NonBlankText = z.string().refine(text => text.trim().length > 0);

export function errorMessage({ error, fallback }: {
  error: unknown;
  fallback: string;
}) {
  const parsed = NonBlankText.safeParse(error);
  if (parsed.success) {
    return parsed.data;
  }

  return error instanceof Error && error.message ? error.message : fallback;
}
