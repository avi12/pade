import { z } from "zod";

import { FeedImage, FeedVideo, WatchedRootEvent } from "@/lib/types/feed";
import { Commit, CommitDetail, PullOutcome, RestoreCandidate, StatusEntry } from "@/lib/types/vcs";

/** One file of one commit, `path` repository-relative as `vcsCommit` lists it. */
interface CommitPathData {
  cwd: string;
  sha: string;
  path: string;
}

export const VCS_COMMANDS = {
  VCS_STATUS: "vcs_status",
  VCS_LOG: "vcs_log",
  VCS_DIFF: "vcs_diff",
  VCS_BRANCHES: "vcs_branches",
  VCS_BRANCH_OF: "vcs_branch_of",
  VCS_REMOTE_BRANCHES: "vcs_remote_branches",
  VCS_SWITCH_BRANCH: "vcs_switch_branch",
  VCS_PULL: "vcs_pull",
  VCS_COMMIT: "vcs_commit",
  VCS_COMMIT_DIFF: "vcs_commit_diff",
  VCS_COMMIT_IMAGE: "vcs_commit_image",
  VCS_COMMIT_VIDEO: "vcs_commit_video",
  VCS_COMMIT_TEXT: "vcs_commit_text",
  VCS_REMOTE_URL: "vcs_remote_url",
  VCS_DEFAULT_BRANCH: "vcs_default_branch",
  VCS_GIT_INSTALLED: "vcs_git_installed",
  VCS_HAS_SSH_KEY: "vcs_has_ssh_key",
  VCS_PROBE_REMOTE: "vcs_probe_remote",
  VCS_CLONE: "vcs_clone",
  VCS_WORKTREE_ADD: "vcs_worktree_add",
  VCS_RESTORE_CANDIDATES: "vcs_restore_candidates",
  VCS_RESTORE_CHECKOUT: "vcs_restore_checkout"
} as const;

export const VCS_EVENTS = {
  GIT_STATE: "git://state"
} as const;

export interface VcsCommandProtocol {
  [VCS_COMMANDS.VCS_STATUS]: (data: { cwd: string }) => StatusEntry[];
  [VCS_COMMANDS.VCS_LOG]: (data: { cwd: string; limit: number }) => Commit[];
  [VCS_COMMANDS.VCS_DIFF]: (data: { cwd: string; path: string; staged: boolean }) => string;
  [VCS_COMMANDS.VCS_BRANCHES]: (data: { cwd: string }) => string[];
  /** Current HEAD branch per project path; non-repo / detached paths are omitted. */
  [VCS_COMMANDS.VCS_BRANCH_OF]: (data: { paths: string[] }) => Record<string, string>;
  /** Branches only a remote has so far, under their short name. */
  [VCS_COMMANDS.VCS_REMOTE_BRANCHES]: (data: { cwd: string }) => string[];
  /** Check `branch` out; answers the branch HEAD ended on, rejects with git's
   *  own refusal. */
  [VCS_COMMANDS.VCS_SWITCH_BRANCH]: (data: { cwd: string; branch: string }) => string;
  /** Fast-forward from `origin` (never a merge commit); `refusedDirty` with
   *  uncommitted tracked changes, rejects when the branch has diverged. */
  [VCS_COMMANDS.VCS_PULL]: (data: { cwd: string }) => PullOutcome;
  /** One commit's message body, per-file stats, and branch. */
  [VCS_COMMANDS.VCS_COMMIT]: (data: { cwd: string; sha: string }) => CommitDetail;
  /** Raw unified diff for one path within a commit. */
  [VCS_COMMANDS.VCS_COMMIT_DIFF]: (data: CommitPathData) => string;
  /** A commit's image as a `data:` URL (a deleted one as its parent had it),
   *  `null` past the size cap. */
  [VCS_COMMANDS.VCS_COMMIT_IMAGE]: (data: CommitPathData) => FeedImage | null;
  /** A commit's video transcoded like `feedVideo`'s. */
  [VCS_COMMANDS.VCS_COMMIT_VIDEO]: (data: CommitPathData) => FeedVideo | null;
  /** A commit's markdown/HTML text, for its rendered Preview. */
  [VCS_COMMANDS.VCS_COMMIT_TEXT]: (data: CommitPathData) => string | null;
  /** The branch-configured remote, falling back to `origin`, as a browser URL. */
  [VCS_COMMANDS.VCS_REMOTE_URL]: (data: { cwd: string }) => string | null;
  /** The configured remote's locally-known default branch, when available. */
  [VCS_COMMANDS.VCS_DEFAULT_BRANCH]: (data: { cwd: string }) => string | null;
  /** Is the `git` CLI installed? Gates the picker's Clone tab. */
  [VCS_COMMANDS.VCS_GIT_INSTALLED]: () => boolean;
  /** Whether an SSH private key exists - without one an SSH clone URL can't
   *  authenticate, so the picker offers HTTPS credentials instead. */
  [VCS_COMMANDS.VCS_HAS_SSH_KEY]: () => boolean;
  /** Can this environment reach the repository (exists + access)? */
  [VCS_COMMANDS.VCS_PROBE_REMOTE]: (data: { url: string }) => boolean;
  /** Clone `url` into `root\name`; answers the new project path. Credentials
   *  switch that one clone to HTTPS and are never persisted. */
  [VCS_COMMANDS.VCS_CLONE]: (data: {
    url: string;
    root: string;
    name: string;
    username?: string;
    password?: string;
  }) => string;
  [VCS_COMMANDS.VCS_WORKTREE_ADD]: (data: { cwd: string; branch: string; create: boolean }) => string;
  /** Rank prior commits by a natural-language description of the version. */
  [VCS_COMMANDS.VCS_RESTORE_CANDIDATES]: (data: { cwd: string; query: string; limit?: number }) => RestoreCandidate[];
  /** Non-destructively check the commit out on a `pade/restore-<sha>` branch. */
  [VCS_COMMANDS.VCS_RESTORE_CHECKOUT]: (data: { cwd: string; sha: string }) => string;
}

export const VCS_RESPONSE_SCHEMAS = {
  [VCS_COMMANDS.VCS_STATUS]: z.array(StatusEntry),
  [VCS_COMMANDS.VCS_LOG]: z.array(Commit),
  [VCS_COMMANDS.VCS_DIFF]: z.string(),
  [VCS_COMMANDS.VCS_BRANCHES]: z.array(z.string()),
  [VCS_COMMANDS.VCS_BRANCH_OF]: z.record(z.string(), z.string()),
  [VCS_COMMANDS.VCS_REMOTE_BRANCHES]: z.array(z.string()),
  [VCS_COMMANDS.VCS_SWITCH_BRANCH]: z.string(),
  [VCS_COMMANDS.VCS_PULL]: PullOutcome,
  [VCS_COMMANDS.VCS_COMMIT]: CommitDetail,
  [VCS_COMMANDS.VCS_COMMIT_DIFF]: z.string(),
  [VCS_COMMANDS.VCS_COMMIT_IMAGE]: FeedImage.nullable(),
  [VCS_COMMANDS.VCS_COMMIT_VIDEO]: FeedVideo.nullable(),
  [VCS_COMMANDS.VCS_COMMIT_TEXT]: z.string().nullable(),
  [VCS_COMMANDS.VCS_REMOTE_URL]: z.string().nullable(),
  [VCS_COMMANDS.VCS_DEFAULT_BRANCH]: z.string().nullable(),
  [VCS_COMMANDS.VCS_GIT_INSTALLED]: z.boolean(),
  [VCS_COMMANDS.VCS_HAS_SSH_KEY]: z.boolean(),
  [VCS_COMMANDS.VCS_PROBE_REMOTE]: z.boolean(),
  [VCS_COMMANDS.VCS_CLONE]: z.string(),
  [VCS_COMMANDS.VCS_WORKTREE_ADD]: z.string(),
  [VCS_COMMANDS.VCS_RESTORE_CANDIDATES]: z.array(RestoreCandidate),
  [VCS_COMMANDS.VCS_RESTORE_CHECKOUT]: z.string()
} satisfies { [Key in keyof VcsCommandProtocol]: z.ZodType<ReturnType<VcsCommandProtocol[Key]>> };

export interface VcsEventProtocol {
  /** A held project's live git state changed (HEAD, config, `git init`, or a
   *  member's). Only the root on purpose: listeners re-fetch what they display. */
  [VCS_EVENTS.GIT_STATE]: (data: WatchedRootEvent) => void;
}

export const VCS_PAYLOAD_SCHEMAS = {
  [VCS_EVENTS.GIT_STATE]: WatchedRootEvent
} satisfies { [Key in keyof VcsEventProtocol]: z.ZodType<Parameters<VcsEventProtocol[Key]>[0]> };
