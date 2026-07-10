// User input is a trust boundary, just like an IPC response. Every free-text
// field / form value is parsed through one of these zod schemas at the point of
// entry — trimmed, length-capped, and shape-checked — before it reaches app
// logic or the backend. One authoritative home for the input schemas (DRY).

import { z } from "zod";

/** Generic trimmed, non-empty text with a sane cap (short single-line fields). */
export const NonEmptyText = z.string().trim().min(1).max(2000);

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
