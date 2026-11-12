import { FeedImage, FeedVideo, WatchedRootEvent } from "@/lib/types/feed";
import {
  Commit,
  CommitDetail,
  PullOutcome,
  RestoreCandidate,
  StatusEntry
} from "@/lib/types/vcs";
import { z } from "zod";

/** One file of one commit, `path` repository-relative as `vcsCommit` lists it. */
type CommitPathData = {
  cwd: string;
  sha: string;
  path: string;
};

export const VCS_COMMANDS = {
  vcsStatus: "vcs_status",
  vcsLog: "vcs_log",
  vcsDiff: "vcs_diff",
  vcsBranches: "vcs_branches",
  vcsBranchOf: "vcs_branch_of",
  vcsRemoteBranches: "vcs_remote_branches",
  vcsSwitchBranch: "vcs_switch_branch",
  vcsPull: "vcs_pull",
  vcsCommit: "vcs_commit",
  vcsCommitDiff: "vcs_commit_diff",
  vcsCommitImage: "vcs_commit_image",
  vcsCommitVideo: "vcs_commit_video",
  vcsCommitText: "vcs_commit_text",
  vcsRemoteUrl: "vcs_remote_url",
  vcsDefaultBranch: "vcs_default_branch",
  vcsGitInstalled: "vcs_git_installed",
  vcsHasSshKey: "vcs_has_ssh_key",
  vcsProbeRemote: "vcs_probe_remote",
  vcsClone: "vcs_clone",
  vcsWorktreeAdd: "vcs_worktree_add",
  vcsRestoreCandidates: "vcs_restore_candidates",
  vcsRestoreCheckout: "vcs_restore_checkout"
} as const;

export const VCS_EVENTS = {
  gitState: "git://state"
} as const;

export interface VcsCommandProtocol {
  [VCS_COMMANDS.vcsStatus]: (data: { cwd: string }) => StatusEntry[];
  [VCS_COMMANDS.vcsLog]: (data: {
    cwd: string;
    limit: number;
  }) => Commit[];
  [VCS_COMMANDS.vcsDiff]: (data: {
    cwd: string;
    path: string;
    staged: boolean;
  }) => string;
  [VCS_COMMANDS.vcsBranches]: (data: { cwd: string }) => string[];
  /** Current HEAD branch per project path; non-repo / detached paths are omitted. */
  [VCS_COMMANDS.vcsBranchOf]: (data: { paths: string[] }) => Record<string, string>;
  /** Branches only a remote has so far, under their short name. */
  [VCS_COMMANDS.vcsRemoteBranches]: (data: { cwd: string }) => string[];
  /** Check `branch` out; answers the branch HEAD ended on, rejects with git's
   *  own refusal. */
  [VCS_COMMANDS.vcsSwitchBranch]: (data: {
    cwd: string;
    branch: string;
  }) => string;
  /** Fast-forward from `origin` (never a merge commit); `refusedDirty` with
   *  uncommitted tracked changes, rejects when the branch has diverged. */
  [VCS_COMMANDS.vcsPull]: (data: { cwd: string }) => PullOutcome;
  /** One commit's message body, per-file stats, and branch. */
  [VCS_COMMANDS.vcsCommit]: (data: {
    cwd: string;
    sha: string;
  }) => CommitDetail;
  /** Raw unified diff for one path within a commit. */
  [VCS_COMMANDS.vcsCommitDiff]: (data: CommitPathData) => string;
  /** A commit's image as a `data:` URL (a deleted one as its parent had it),
   *  `null` past the size cap. */
  [VCS_COMMANDS.vcsCommitImage]: (data: CommitPathData) => FeedImage | null;
  /** A commit's video transcoded like `feedVideo`'s. */
  [VCS_COMMANDS.vcsCommitVideo]: (data: CommitPathData) => FeedVideo | null;
  /** A commit's markdown/HTML text, for its rendered Preview. */
  [VCS_COMMANDS.vcsCommitText]: (data: CommitPathData) => string | null;
  /** The branch-configured remote, falling back to `origin`, as a browser URL. */
  [VCS_COMMANDS.vcsRemoteUrl]: (data: { cwd: string }) => string | null;
  /** The configured remote's locally-known default branch, when available. */
  [VCS_COMMANDS.vcsDefaultBranch]: (data: { cwd: string }) => string | null;
  /** Is the `git` CLI installed? Gates the picker's Clone tab. */
  [VCS_COMMANDS.vcsGitInstalled]: () => boolean;
  /** Whether an SSH private key exists - without one an SSH clone URL can't
   *  authenticate, so the picker offers HTTPS credentials instead. */
  [VCS_COMMANDS.vcsHasSshKey]: () => boolean;
  /** Can this environment reach the repository (exists + access)? */
  [VCS_COMMANDS.vcsProbeRemote]: (data: { url: string }) => boolean;
  /** Clone `url` into `root\name`; answers the new project path. Credentials
   *  switch that one clone to HTTPS and are never persisted. */
  [VCS_COMMANDS.vcsClone]: (data: {
    url: string;
    root: string;
    name: string;
    username?: string;
    password?: string;
  }) => string;
  [VCS_COMMANDS.vcsWorktreeAdd]: (data: {
    cwd: string;
    branch: string;
    create: boolean;
  }) => string;
  /** Rank prior commits by a natural-language description of the version. */
  [VCS_COMMANDS.vcsRestoreCandidates]: (data: {
    cwd: string;
    query: string;
    limit?: number;
  }) => RestoreCandidate[];
  /** Non-destructively check the commit out on a `pade/restore-<sha>` branch. */
  [VCS_COMMANDS.vcsRestoreCheckout]: (data: {
    cwd: string;
    sha: string;
  }) => string;
}

export const VCS_RESPONSE_SCHEMAS = {
  [VCS_COMMANDS.vcsStatus]: z.array(StatusEntry),
  [VCS_COMMANDS.vcsLog]: z.array(Commit),
  [VCS_COMMANDS.vcsDiff]: z.string(),
  [VCS_COMMANDS.vcsBranches]: z.array(z.string()),
  [VCS_COMMANDS.vcsBranchOf]: z.record(z.string(), z.string()),
  [VCS_COMMANDS.vcsRemoteBranches]: z.array(z.string()),
  [VCS_COMMANDS.vcsSwitchBranch]: z.string(),
  [VCS_COMMANDS.vcsPull]: PullOutcome,
  [VCS_COMMANDS.vcsCommit]: CommitDetail,
  [VCS_COMMANDS.vcsCommitDiff]: z.string(),
  [VCS_COMMANDS.vcsCommitImage]: FeedImage.nullable(),
  [VCS_COMMANDS.vcsCommitVideo]: FeedVideo.nullable(),
  [VCS_COMMANDS.vcsCommitText]: z.string().nullable(),
  [VCS_COMMANDS.vcsRemoteUrl]: z.string().nullable(),
  [VCS_COMMANDS.vcsDefaultBranch]: z.string().nullable(),
  [VCS_COMMANDS.vcsGitInstalled]: z.boolean(),
  [VCS_COMMANDS.vcsHasSshKey]: z.boolean(),
  [VCS_COMMANDS.vcsProbeRemote]: z.boolean(),
  [VCS_COMMANDS.vcsClone]: z.string(),
  [VCS_COMMANDS.vcsWorktreeAdd]: z.string(),
  [VCS_COMMANDS.vcsRestoreCandidates]: z.array(RestoreCandidate),
  [VCS_COMMANDS.vcsRestoreCheckout]: z.string()
} satisfies { [Key in keyof VcsCommandProtocol]: z.ZodType<ReturnType<VcsCommandProtocol[Key]>> };

export interface VcsEventProtocol {
  /** A held project's live git state changed (HEAD, config, `git init`, or a
   *  member's). Only the root on purpose: listeners re-fetch what they display. */
  [VCS_EVENTS.gitState]: (data: WatchedRootEvent) => void;
}

export const VCS_PAYLOAD_SCHEMAS = {
  [VCS_EVENTS.gitState]: WatchedRootEvent
} satisfies { [Key in keyof VcsEventProtocol]: z.ZodType<Parameters<VcsEventProtocol[Key]>[0]> };
