// User input is a trust boundary, just like an IPC response. Every free-text
// field / form value is parsed through one of these zod schemas at the point of
// entry — trimmed, length-capped, and shape-checked — before it reaches app
// logic or the backend. One authoritative home for the input schemas (DRY).

import { z } from "zod";

/** A restore-a-version query — a short natural-language description. */
export const RestoreQuery = z.string().trim().min(1).max(200);

/** A project / workspace name — no path separators or Windows-reserved chars
 *  (hyphens/underscores/dots are fine, e.g. an auto-named "brave-otter"). */
export const ProjectName = z
  .string()
  .trim()
  .min(1)
  .max(100)
  .regex(/^[^\\/:*?"<>|]+$/, "Name can't contain path characters.");

/** A folder path the user typed or pasted. */
export const FolderPath = z.string().trim().min(1).max(4096);

/** A session tab's display name — a short single-line label. */
export const SessionName = z.string().trim().min(1).max(60);

/** An optional first prompt seeded to an agent — may be empty, but capped. */
export const FirstPrompt = z.string().trim().max(10_000);

/** Parse a user input without throwing: returns the validated value, or null if
 *  it fails the schema (e.g. empty → no-op). Use `schema.safeParse` directly when
 *  you need the specific error message to show the user. */
export function parseInput<T>({ schema, raw }: {
  schema: z.ZodType<T>;
  raw: unknown;
}): T | null {
  const result = schema.safeParse(raw);
  return result.success ? result.data : null;
}

/** Live validation for a name field (create/rename): the schema's own message
 *  for a non-empty invalid name (e.g. "Name can't contain path characters."),
 *  or null. An empty field yields no message — nothing typed yet — so callers
 *  gate their submit on the schema separately. */
export function nameError(raw: string): string | null {
  if (raw.trim().length === 0) {
    return null;
  }

  const result = ProjectName.safeParse(raw);
  return result.success ? null : result.error.issues[0].message;
}
