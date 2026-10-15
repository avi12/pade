// User input is a trust boundary, just like an IPC response. Every free-text
// field / form value is parsed through one of these zod schemas at the point of
// entry - trimmed, length-capped, and shape-checked - before it reaches app
// logic or the backend. One authoritative home for the input schemas (DRY).

import { MAXIMUM_HANDOFF_PERCENTAGE, MINIMUM_HANDOFF_PERCENTAGE } from "@/lib/usage/context-level";
import { z } from "zod";

const MAXIMUM_QUERY_LENGTH = 200;
const MAXIMUM_PROJECT_NAME_LENGTH = 100;
const MAXIMUM_PATH_LENGTH = 4096;
/** Mirrors the backend's `MAXIMUM_MANUAL_LABEL_CHARACTERS` for workspace labels. */
const MAXIMUM_LABEL_LENGTH = 60;
const MAXIMUM_URL_LENGTH = 2048;
const MAXIMUM_USERNAME_LENGTH = 200;
const MAXIMUM_SECRET_LENGTH = 500;
const GIT_SUFFIX = ".git";

/** The auto-handoff threshold as typed/slid in Config - a whole percent inside
 *  the allowed band (the band lives in lib/usage/context-level, the SSOT). */
export const HandoffPercent = z.coerce
  .number()
  .int()
  .min(MINIMUM_HANDOFF_PERCENTAGE)
  .max(MAXIMUM_HANDOFF_PERCENTAGE);

/** A restore-a-version query - a short natural-language description. */
export const RestoreQuery = z.string().trim().min(1).max(MAXIMUM_QUERY_LENGTH);

/** A project / workspace name - no path separators or Windows-reserved chars
 *  (hyphens/underscores/dots are fine, e.g. an auto-named "brave-otter"). */
export const ProjectName = z
  .string()
  .trim()
  .min(1)
  .max(MAXIMUM_PROJECT_NAME_LENGTH)
  .regex(/^[^\\/:*?"<>|]+$/, "Name can't contain path characters");

/** A folder path the user typed or pasted. */
export const FolderPath = z.string().trim().min(1).max(MAXIMUM_PATH_LENGTH);

/** A session tab's display name - a short single-line label. */
export const SessionName = z.string().trim().min(1).max(MAXIMUM_LABEL_LENGTH);

/** A name the user gives a workspace inside PADE (its folder keeps its own name):
 *  any text on one line, so unlike `ProjectName` path characters and non-Latin
 *  script are fine. The backend (`workspace::manual_label`) enforces the same. */
export const WorkspaceLabel = z
  .string()
  .trim()
  .min(1, "Give it a label")
  .max(MAXIMUM_LABEL_LENGTH, `Keep it under ${MAXIMUM_LABEL_LENGTH} characters`)
  .regex(/^\P{Cc}+$/u, "A label is a single line");

/** What the terminal's find bar searches for - kept verbatim (no trim: a leading
 *  or trailing space is a legitimate thing to look for in output), capped so a
 *  pasted file can't be handed to the search engine keystroke by keystroke. */
export const TerminalFindTerm = z.string().min(1).max(MAXIMUM_QUERY_LENGTH);

// ── Clone URLs - the one home for their shape knowledge (schema + helpers). ──

/** The URL-proper forms git clones over, validated by zod's own URL parser and
 *  pinned to git's transports. Git's scp-like `git@host:path` is NOT a URL, so
 *  it's recognised separately (`isScpLike`) rather than forced through here. */
const StandardCloneUrl = z
  .url({ protocol: /^(?:https|ssh)$/ })
  .refine(url => {
    if (!URL.canParse(url)) {
      return false;
    }

    const parsed = new URL(url);
    return !parsed.password && (parsed.protocol !== "https:" || !parsed.username);
  });

/** Scp-like `git@host:path` - a `user@host` head before the first colon. */
function isScpLike(url: string) {
  const iColon = url.indexOf(":");
  return iColon > 0 && iColon < url.length - 1 && url.slice(0, iColon).includes("@");
}

/** A git clone URL over an authenticated transport: HTTPS, SSH, or scp-like SSH. */
export const CloneUrl = z
  .string()
  .trim()
  .min(1)
  .max(MAXIMUM_URL_LENGTH)
  .refine(url => StandardCloneUrl.safeParse(url).success || isScpLike(url), {
    message: "Enter an https://, ssh://, or git@host:path URL"
  });

/** A git username / email for an HTTPS clone. */
export const GitUsername = z.string().trim().min(1).max(MAXIMUM_USERNAME_LENGTH);

/** A git password / access token - kept verbatim (no trim: it's a secret). */
export const GitSecret = z.string().min(1).max(MAXIMUM_SECRET_LENGTH);

/** Whether a clone URL authenticates over SSH (`ssh://` or scp-like `git@…`). */
export function isSshCloneUrl(url: string) {
  const trimmed = url.trim();
  return trimmed.startsWith("ssh://") || isScpLike(trimmed);
}

/** The folder name a clone URL suggests - its last path segment minus `.git` -
 *  or "" while the URL doesn't resolve to a valid project name yet. */
export function repositoryFolderName(url: string) {
  const trimmed = url.trim().replace(/[/\\]+$/, "");
  const isStandardUrl = StandardCloneUrl.safeParse(trimmed).success;
  if (!isStandardUrl && !isScpLike(trimmed)) {
    return "";
  }

  // A URL proper carries the repo in its pathname; the scp form after its colon.
  const path = isStandardUrl ? new URL(trimmed).pathname : (trimmed.split(":").at(-1) ?? "");
  const lastSegment = path.split("/").at(-1) ?? "";
  const name = lastSegment.endsWith(GIT_SUFFIX) ? lastSegment.slice(0, -GIT_SUFFIX.length) : lastSegment;
  return ProjectName.safeParse(name).success ? name : "";
}

/** An optional first prompt seeded to an agent - may be empty, and uncapped: it
 *  is the user's own instructions, written (typed) straight into the agent's
 *  input rather than passed as an argument, so a long detailed spec is exactly
 *  the point and no length limit applies. */
export const FirstPrompt = z.string().trim();

/** Parse a user input without throwing: returns the validated value, or null if
 *  it fails the schema (e.g. empty → no-op). Use `schema.safeParse` directly when
 *  you need the specific error message to show the user. */
export function parseInput<T>({ schema, raw }: {
  schema: z.ZodType<T>;
  raw: unknown;
}) {
  const parsed = schema.safeParse(raw);
  return parsed.success ? parsed.data : null;
}

/** Live validation for a text field: the schema's own message for a non-empty
 *  invalid value, or null. An empty field yields no message - nothing typed yet -
 *  so callers gate their submit on the schema separately. */
export function fieldError({ schema, raw }: {
  schema: z.ZodType;
  raw: string;
}) {
  if (raw.trim().length === 0) {
    return null;
  }

  const parsed = schema.safeParse(raw);
  return parsed.success ? null : parsed.error.issues[0].message;
}

/** Live validation for a name field (create/rename), e.g. "Name can't contain
 *  path characters" - see `fieldError`. */
export function nameError(raw: string) {
  return fieldError({
    schema: ProjectName,
    raw
  });
}
