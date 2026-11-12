import { z } from "zod";

export const VcsKind = z.enum(["created", "modified", "deleted", "renamed", "untracked"]);

export type VcsKind = z.infer<typeof VcsKind>;

export const StatusEntry = z.object({
  path: z.string(),
  kind: VcsKind,
  staged: z.boolean()
});

export type StatusEntry = z.infer<typeof StatusEntry>;

export const Commit = z.object({
  id: z.string(),
  short: z.string(),
  summary: z.string(),
  author: z.string(),
  when: z.string(),
  /** Lines added across the commit (defaulted for back-compat with old callers). */
  additions: z.number().default(0),
  /** Lines deleted across the commit. */
  deletions: z.number().default(0),
  /** Files the commit touched. */
  files: z.number().default(0)
});

export type Commit = z.infer<typeof Commit>;

/** One file changed by a commit, with its per-file line counts. */
const CommitFileEntry = z.object({
  path: z.string(),
  kind: VcsKind,
  additions: z.number(),
  deletions: z.number(),
  /** True when git reports the file as binary (line counts are meaningless). */
  binary: z.boolean().default(false)
});

export type CommitFileEntry = z.infer<typeof CommitFileEntry>;

/** A single commit's full detail: message body, branch, and per-file stats.
 *  Reuses `Commit`'s header fields. */
export const CommitDetail = Commit.pick({
  id: true,
  short: true,
  summary: true,
  author: true,
  when: true
}).extend({
  /** The commit message body (everything after the subject); empty if none. */
  body: z.string(),
  /** Current HEAD branch name (empty on a detached HEAD). */
  branch: z.string(),
  files: z.array(CommitFileEntry),
  additions: z.number(),
  deletions: z.number()
});

export type CommitDetail = z.infer<typeof CommitDetail>;

/** A commit ranked as a candidate for "restore a version" - a Commit plus a
 *  0..≈1.5 fuzzy `score` (token overlap with the query, boosted by time hints). */
export const RestoreCandidate = Commit.extend({ score: z.number() });

export type RestoreCandidate = z.infer<typeof RestoreCandidate>;

/** How a "Sync all" fast-forward pull resolved. `updated` fast-forwarded to new
 *  upstream commits, `alreadyUpToDate` had nothing to fetch, and `refusedDirty`
 *  bailed because the working tree has uncommitted changes to tracked files. A
 *  branch that diverged from upstream (no fast-forward) throws instead. */
const PullStatus = z.enum(["updated", "alreadyUpToDate", "refusedDirty"]);

/** The result of `vcs_pull`: the outcome plus a short human line for the toast. */
export const PullOutcome = z.object({
  status: PullStatus,
  message: z.string()
});

export type PullOutcome = z.infer<typeof PullOutcome>;
