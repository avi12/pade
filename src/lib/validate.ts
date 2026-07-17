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

// ── Clone URLs — the one home for their shape knowledge (schema + helpers). ──

/** The URL-proper forms git clones over, validated by zod's own URL parser and
 *  pinned to git's transports. Git's scp-like `git@host:path` is NOT a URL, so
 *  it's recognised separately (`isScpLike`) rather than forced through here. */
const StandardCloneUrl = z.url({ protocol: /^(?:https?|ssh|git)$/ });

/** Scp-like `git@host:path` — a `user@host` head before the first colon. */
function isScpLike(url: string): boolean {
  const colon = url.indexOf(":");
  return colon > 0 && colon < url.length - 1 && url.slice(0, colon).includes("@");
}

/** A git clone URL — `https://`, `ssh://`, `git://`, or scp-like `git@host:path`. */
export const CloneUrl = z
  .string()
  .trim()
  .min(1)
  .max(2048)
  .refine(url => StandardCloneUrl.safeParse(url).success || isScpLike(url), {
    message: "Enter an https://, ssh://, git://, or git@host:path URL."
  });

/** A git username / email for an HTTPS clone. */
export const GitUsername = z.string().trim().min(1).max(200);

/** A git password / access token — kept verbatim (no trim: it's a secret). */
export const GitSecret = z.string().min(1).max(500);

/** Whether a clone URL authenticates over SSH (`ssh://` or scp-like `git@…`). */
export function isSshCloneUrl(url: string): boolean {
  const trimmed = url.trim();
  return trimmed.startsWith("ssh://") || isScpLike(trimmed);
}

/** The folder name a clone URL suggests — its last path segment minus `.git` —
 *  or "" while the URL doesn't resolve to a valid project name yet. */
export function repoFolderName(url: string): string {
  const trimmed = url.trim().replace(/[/\\]+$/, "");
  const isStandardUrl = StandardCloneUrl.safeParse(trimmed).success;
  if (!isStandardUrl && !isScpLike(trimmed)) {
    return "";
  }

  // A URL proper carries the repo in its pathname; the scp form after its colon.
  const path = isStandardUrl ? new URL(trimmed).pathname : (trimmed.split(":").at(-1) ?? "");
  const lastSegment = path.split("/").at(-1) ?? "";
  const name = lastSegment.endsWith(".git") ? lastSegment.slice(0, -".git".length) : lastSegment;
  return ProjectName.safeParse(name).success ? name : "";
}

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
