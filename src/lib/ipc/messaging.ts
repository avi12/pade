// The single boundary between the UI and the Rust core (DRY + SoC), shaped like
// `@webext-core/messaging`: every `#[tauri::command]` is a `Command` member and
// every event channel an `EventType` member; the protocol maps say what each
// takes and returns, and the zod schemas below are the single source of truth
// for what comes back. Callers use `sendMessage(Command.x, data)` and
// `onMessage(EventType.x, ({ data }) => ...)` directly.

import { defineTauriMessaging } from "@/lib/ipc/define-tauri-messaging";
import {
  AccountUsage,
  AddRootOutcome,
  Agent,
  ChangeEvent,
  Commit,
  CommitDetail,
  ConfigFile,
  DesignTool,
  DragDropPayload,
  DragOverPayload,
  EditorKind,
  FeedDiff,
  FeedImage,
  FeedVideo,
  Ide,
  LanguageShare,
  LaunchContext,
  LimitReset,
  LimitResetOutcome,
  McpChange,
  ModelOption,
  PathProbe,
  ProjectEntry,
  PtyChunk,
  PtyExit,
  PtyHistory,
  PtySession,
  PullOutcome,
  RestoreCandidate,
  RunnerData,
  RunnerExit,
  RunnerInfo,
  RunningModel,
  RunningModelChanged,
  SaveMigration,
  Scheme,
  SessionContext,
  Settings,
  StatusEntry,
  SwitchOutcome,
  TaskGroup,
  TaskManifestDescriptor,
  TerminalScheme,
  Usage,
  WatchStatus,
  WindowInfo,
  WorkspaceMember
} from "@/lib/types";
import type {
  LabelSource,
  ModelChoice,
  Prefs,
  WindowDirection,
  WindowMode
} from "@/lib/types";
import { z } from "zod";

/** Every Rust `#[tauri::command]`, valued by its exact registered name. The
 *  drift guard (`messaging.test.ts`) holds this set equal to `lib.rs`'s
 *  `generate_handler!` list. */
export enum Command {
  agentsDetect = "agents_detect",
  agentThemePublish = "agent_theme_publish",
  agentModelOptions = "agent_model_options",
  agentApplyModel = "agent_apply_model",
  agentResumeArgs = "agent_resume_args",
  agentSessionContext = "agent_session_context",
  agentRunningModel = "agent_running_model",
  projectAutoname = "project_autoname",
  sessionGenerateName = "session_generate_name",
  sessionSummarize = "session_summarize",
  ideDetect = "ide_detect",
  ideSuggest = "ide_suggest",
  ideChooseEditor = "ide_choose_editor",
  ideKinds = "ide_kinds",
  ideKindOptions = "ide_kind_options",
  ideProjectKinds = "ide_project_kinds",
  ideProjectLanguages = "ide_project_languages",
  ideAddEditor = "ide_add_editor",
  ideRemoveEditor = "ide_remove_editor",
  ideOpen = "ide_open",
  ideOpenFile = "ide_open_file",
  openInExplorer = "open_in_explorer",
  revealInExplorer = "reveal_in_explorer",
  openInTerminal = "open_in_terminal",
  openUrl = "open_url",
  restart = "restart",
  clipboardImageSave = "clipboard_image_save",
  recoveryArm = "recovery_arm",
  windowCreate = "window_create",
  windowRegisterProject = "window_register_project",
  windowFocusProject = "window_focus_project",
  windowFocusRelative = "window_focus_relative",
  windowList = "window_list",
  windowReorder = "window_reorder",
  windowFocusLabel = "window_focus_label",
  windowTheme = "window_theme",
  designTools = "design_tools",
  designOpen = "design_open",
  discordSetActivity = "discord_set_activity",
  discordClearActivity = "discord_clear_activity",
  contextMenuStatus = "context_menu_status",
  contextMenuRegister = "context_menu_register",
  contextMenuUnregister = "context_menu_unregister",
  ptySpawn = "pty_spawn",
  ptyWrite = "pty_write",
  ptyResize = "pty_resize",
  ptyKill = "pty_kill",
  ptyList = "pty_list",
  ptyHistory = "pty_history",
  sessionTaskStop = "session_task_stop",
  sessionTaskRunning = "session_task_running",
  watchStart = "watch_start",
  watchStatus = "watch_status",
  watchDirs = "watch_dirs",
  watchLanguages = "watch_languages",
  feedDiff = "feed_diff",
  feedImage = "feed_image",
  feedVideo = "feed_video",
  feedText = "feed_text",
  feedIgnored = "feed_ignored",
  membersList = "members_list",
  vcsStatus = "vcs_status",
  vcsLog = "vcs_log",
  vcsDiff = "vcs_diff",
  vcsBranches = "vcs_branches",
  vcsBranchOf = "vcs_branch_of",
  vcsRemoteBranches = "vcs_remote_branches",
  vcsSwitchBranch = "vcs_switch_branch",
  vcsPull = "vcs_pull",
  vcsCommit = "vcs_commit",
  vcsCommitDiff = "vcs_commit_diff",
  vcsRemoteUrl = "vcs_remote_url",
  vcsDefaultBranch = "vcs_default_branch",
  vcsGitInstalled = "vcs_git_installed",
  vcsHasSshKey = "vcs_has_ssh_key",
  vcsProbeRemote = "vcs_probe_remote",
  vcsClone = "vcs_clone",
  vcsWorktreeAdd = "vcs_worktree_add",
  vcsRestoreCandidates = "vcs_restore_candidates",
  vcsRestoreCheckout = "vcs_restore_checkout",
  runnerStart = "runner_start",
  runnerStop = "runner_stop",
  runnerList = "runner_list",
  tasksDescriptors = "tasks_descriptors",
  tasksList = "tasks_list",
  usageGet = "usage_get",
  usageAccountAgent = "usage_account_agent",
  limitResetOffer = "limit_reset_offer",
  limitResetClaim = "limit_reset_claim",
  configList = "config_list",
  configRead = "config_read",
  terminalSchemes = "terminal_schemes",
  launchContext = "launch_context",
  settingsGet = "settings_get",
  workspaceAddRoot = "workspace_add_root",
  workspaceRemoveRoot = "workspace_remove_root",
  workspaceProbePath = "workspace_probe_path",
  workspaceScan = "workspace_scan",
  workspaceOpen = "workspace_open",
  handoffDocDelete = "handoff_doc_delete",
  workspaceTemp = "workspace_temp",
  workspaceMove = "workspace_move",
  workspaceRename = "workspace_rename",
  workspaceRenameFolder = "workspace_rename_folder",
  workspaceSaveMigrate = "workspace_save_migrate",
  workspaceSetLabel = "workspace_set_label",
  workspaceDelete = "workspace_delete",
  workspacePrune = "workspace_prune",
  workspaceCreate = "workspace_create",
  workspaceClearRecent = "workspace_clear_recent",
  workspaceSetPinned = "workspace_set_pinned",
  workspaceRemoveRecent = "workspace_remove_recent",
  workspaceSetPinnedOrder = "workspace_set_pinned_order",
  workspaceDeleteDirectory = "workspace_delete_directory",
  setDefaultAgent = "set_default_agent",
  setProjectAgent = "set_project_agent",
  setPrefs = "set_prefs"
}

/** Every event channel the frontend listens to, valued by its wire name. The
 *  drift guard holds each one to an emit in `src-tauri/src` (the `tauri://`
 *  ones are Tauri's own). */
export enum EventType {
  ptyData = "pty://data",
  ptyExit = "pty://exit",
  agentModelChanged = "agent-model://changed",
  feedChange = "feed://change",
  feedIgnoreChanged = "feed://ignore-changed",
  mcpChanged = "mcp://changed",
  directoriesChanged = "directories://changed",
  languagesChanged = "languages://changed",
  gitState = "git://state",
  runnerData = "runner://data",
  runnerExit = "runner://exit",
  windowsChanged = "windows://changed",
  settingsChanged = "settings://changed",
  themeChanged = "theme://changed",
  dragOver = "tauri://drag-over",
  dragDrop = "tauri://drag-drop",
  dragLeave = "tauri://drag-leave"
}

/** Rust's unit `()` - a command that answers nothing - arrives as JSON null,
 *  which callers see as `void`. */
const NO_REPLY = z.null().transform(() => undefined);

/** A path the backend previews or acts on. */
type PathData = { path: string };

/** A workspace moved under a new folder name, optionally into another saved
 *  root (omitted → the primary root). */
type WorkspaceRename = {
  from: string;
  newName: string;
  root?: string;
};

/** A task the agent started itself, addressed by its session and command line. */
type SessionTask = {
  id: string;
  command: string;
};

export interface CommandProtocolMap {
  [Command.agentsDetect]: () => Agent[];
  /** Re-theme every RUNNING session of a live-themed agent (Claude Code) by
   *  rewriting the theme definition it watches. Sent on every scheme flip: the
   *  agent repaints itself, so a conversation in flight never restarts to
   *  follow the app's appearance (see theming.rs). */
  [Command.agentThemePublish]: (data: { scheme: Scheme }) => void;
  /** The models `command` offers to switch to, from the agent's own local
   *  catalog. Empty for an agent PADE can't read a catalog for. */
  [Command.agentModelOptions]: (data: { command: string }) => ModelOption[];
  /** Apply a model / effort picked from the tab to the running session.
   *  `applied`: switched in place (Claude). `needsRestart`: the agent only
   *  takes a model at launch - relaunch it on its conversation. */
  [Command.agentApplyModel]: (data: {
    sessionId: string;
    command: string;
    conversationId: string;
    choice: ModelChoice;
  }) => SwitchOutcome;
  /** Launch args that reopen the agent's recorded conversation for `cwd`
   *  (`["resume", "<uuid>"]` for Codex), empty when the agent has no such
   *  mechanism or no recording. `conversationId`: when the agent reported its
   *  own session id for it, that exact session is resumed. */
  [Command.agentResumeArgs]: (data: {
    command: string;
    cwd: string;
    conversationId?: string;
  }) => string[];
  /** How full the session's context window is, read from the agent's own
   *  session log; `null` for an agent that keeps no readable log. */
  [Command.agentSessionContext]: (data: {
    command: string;
    conversationId?: string;
  }) => SessionContext | null;
  /** The model and effort the session runs, as the agent itself reported it
   *  (hooks, its session log, a plugin - never its terminal). */
  [Command.agentRunningModel]: (data: {
    command: string;
    conversationId: string;
  }) => RunningModel | null;
  /** Suggest a name for a temp workspace via the agent CLI, else a heuristic. */
  [Command.projectAutoname]: (data: {
    path: string;
    agent: string;
  }) => string | null;
  /** A concise session name from its transcript (null until there's enough
   *  conversation, or if nothing usable is produced). */
  [Command.sessionGenerateName]: (data: {
    id: string;
    agent: string;
  }) => string | null;
  /** A sentence or two on what the session has done and is doing now, from its
   *  transcript, for the agents dashboard (null until there's enough). */
  [Command.sessionSummarize]: (data: {
    id: string;
    agent: string;
    /** The session's folder, whose branch's open pull request the summary links. */
    cwd: string;
  }) => string | null;
  [Command.ideDetect]: () => Ide[];
  /** Installed IDEs ordered for the given project: explicit per-project choice
   *  → rule → fallback → language-coverage ranking. `ides[0]` is *the* editor
   *  on every surface. */
  [Command.ideSuggest]: (data: { cwd: string }) => Ide[];
  /** Persist an explicit editor pick for the project at `cwd`. */
  [Command.ideChooseEditor]: (data: {
    cwd: string;
    id: string;
  }) => Settings;
  /** The project kinds the rules engine shows, in render/priority order. */
  [Command.ideKinds]: () => EditorKind[];
  /** Editor ids suited to each project kind (kind → ordered, installed-only). */
  [Command.ideKindOptions]: () => Record<string, string[]>;
  /** Primary detected kind per project path; unrecognised paths are omitted. */
  [Command.ideProjectKinds]: (data: { paths: string[] }) => Record<string, string>;
  /** Each project's languages by share of its source, biggest first;
   *  paths with no countable source are omitted. */
  [Command.ideProjectLanguages]: (data: { paths: string[] }) => Record<string, LanguageShare[]>;
  /** Add an editor by its executable path; rejects with the message when it
   *  isn't a supported editor. */
  [Command.ideAddEditor]: (data: PathData) => Settings;
  [Command.ideRemoveEditor]: (data: { id: string }) => Settings;
  /** `line` is 1-based, meaningful only when `path` is a file. */
  [Command.ideOpen]: (data: {
    command: string;
    path?: string;
    line?: number;
  }) => void;
  /** Open a file inside its `project` workspace window, jumping to `line`. */
  [Command.ideOpenFile]: (data: {
    command: string;
    project: string;
    file: string;
    line?: number;
  }) => void;
  [Command.openInExplorer]: (data: PathData) => void;
  /** Show a file selected in its folder - never opens or runs the file itself. */
  [Command.revealInExplorer]: (data: PathData) => void;
  [Command.openInTerminal]: (data: PathData) => void;
  /** Open an http(s) URL in the system's default browser. */
  [Command.openUrl]: (data: { url: string }) => void;
  /** Restart PADE so launch-only settings can take effect. */
  [Command.restart]: () => void;
  /** When the clipboard holds a bitmap, save it as a PNG in a bounded temp
   *  folder and answer its absolute path; null when text-only. */
  [Command.clipboardImageSave]: () => string | null;
  /** Arm WebView2 crash auto-recovery with this window's live URL - the one
   *  value a dead browser process can no longer report. */
  [Command.recoveryArm]: (data: { url: string }) => void;
  /** Spawn a fresh app window targeting a project, an empty picker, or a
   *  throwaway workspace; it routes off its query string. */
  [Command.windowCreate]: (data: {
    mode: WindowMode;
    path?: string;
  }) => void;
  /** Record the project this window now has open (focus-instead-of-reopen). */
  [Command.windowRegisterProject]: (data: PathData) => void;
  /** Focus another window already showing this project; true if one was found. */
  [Command.windowFocusProject]: (data: PathData) => boolean;
  /** Focus the previous/next window in stable creation order; false when this
   *  is the only one. */
  [Command.windowFocusRelative]: (data: { direction: WindowDirection }) => boolean;
  /** Every open window that has a project, in the user's explicit order. */
  [Command.windowList]: () => WindowInfo[];
  /** Persist the drag-reordered "Open windows" order - the one source both
   *  that list and the Ctrl+Alt+[ / ] cycle read. */
  [Command.windowReorder]: (data: { labels: string[] }) => void;
  /** Focus a specific open window by its label; true if it existed. */
  [Command.windowFocusLabel]: (data: { label: string }) => boolean;
  /** This window's OS theme from the native side - the authoritative answer at
   *  startup, where WebView2's media query can still say light (window.rs). */
  [Command.windowTheme]: () => Scheme;
  /** AI design/UI-generation tools, ranked for the active agent. */
  [Command.designTools]: (data: { agent: string }) => DesignTool[];
  /** Open the picked tool in the companion PADE window (iframes are blocked). */
  [Command.designOpen]: (data: { id: string }) => void;
  /** Report PADE on the user's Discord profile. `image` is the small overlay
   *  art-asset key, `caption` its hover text. */
  [Command.discordSetActivity]: (data: {
    details?: string;
    state?: string;
    image?: string;
    caption?: string;
  }) => void;
  [Command.discordClearActivity]: () => void;
  /** Whether the Explorer "Open in PADE" folder context-menu entry is registered. */
  [Command.contextMenuStatus]: () => boolean;
  [Command.contextMenuRegister]: () => void;
  [Command.contextMenuUnregister]: () => void;
  /** `args`: extra args for `command`. `scheme`: the resolved appearance, so
   *  env-themed CLIs start matching it. `conversationId`: the conversation to
   *  pin/resume (`claude --session-id`). `modelChoice`: launch flags for this
   *  session alone, never the agent's saved default. */
  [Command.ptySpawn]: (data: {
    id: string;
    command: string | null;
    cwd: string | null;
    cols: number;
    rows: number;
    args?: string[];
    scheme?: Scheme;
    conversationId?: string;
    modelChoice?: ModelChoice;
  }) => void;
  [Command.ptyWrite]: (data: {
    id: string;
    data: string;
  }) => void;
  [Command.ptyResize]: (data: {
    id: string;
    cols: number;
    rows: number;
  }) => void;
  [Command.ptyKill]: (data: { id: string }) => void;
  /** Every live session the backend still hosts - the roster a reloaded window
   *  re-attaches from (see `session-restore`). */
  [Command.ptyList]: () => PtySession[];
  /** What a terminal attaching mid-flight needs to paint, and the sequence
   *  number of the last chunk already inside it. */
  [Command.ptyHistory]: (data: { id: string }) => PtyHistory;
  /** Stop a task the agent started itself; true when a matching process was found. */
  [Command.sessionTaskStop]: (data: SessionTask) => boolean;
  /** Whether a task the agent started is still running under its tree. */
  [Command.sessionTaskRunning]: (data: SessionTask) => boolean;
  /** Watch `root` - the open workspace - so the feed follows the project on
   *  screen. Idempotent per root; a new root re-roots the watcher. */
  [Command.watchStart]: (data: { root: string }) => void;
  /** What this window's watch is doing right now, for the feed's empty panel. */
  [Command.watchStatus]: () => WatchStatus;
  /** Arm the picker's folder watcher on the parents of the rows on the page. */
  [Command.watchDirs]: (data: { dirs: string[] }) => void;
  /** Watch the `.git` folders of the project rows on screen, so a row whose
   *  tracked files change re-counts its languages; an empty list stops. */
  [Command.watchLanguages]: (data: { paths: string[] }) => void;
  /** The card's git-free preview: the first-touch baseline vs current content. */
  [Command.feedDiff]: (data: PathData) => FeedDiff | null;
  /** An image path's current bytes as a `data:` URL, `null` past the size cap. */
  [Command.feedImage]: (data: PathData) => FeedImage | null;
  /** A video path's opening seconds transcoded to an MP4 `data:` URL, or
   *  `noTranscoder` without ffmpeg. */
  [Command.feedVideo]: (data: PathData) => FeedVideo | null;
  /** The current text of a watched markdown/HTML path, for the card's Preview. */
  [Command.feedText]: (data: PathData) => string | null;
  /** The subset of `paths` the current ignore policy excludes. */
  [Command.feedIgnored]: (data: { paths: string[] }) => string[];
  /** Confirmed members of the workspace at `root`; the root itself is first. */
  [Command.membersList]: (data: { root: string }) => WorkspaceMember[];
  [Command.vcsStatus]: (data: { cwd: string }) => StatusEntry[];
  [Command.vcsLog]: (data: {
    cwd: string;
    limit: number;
  }) => Commit[];
  [Command.vcsDiff]: (data: {
    cwd: string;
    path: string;
    staged: boolean;
  }) => string;
  [Command.vcsBranches]: (data: { cwd: string }) => string[];
  /** Current HEAD branch per project path; non-repo / detached paths are omitted. */
  [Command.vcsBranchOf]: (data: { paths: string[] }) => Record<string, string>;
  /** Branches only a remote has so far, under their short name. */
  [Command.vcsRemoteBranches]: (data: { cwd: string }) => string[];
  /** Check `branch` out; answers the branch HEAD ended on, rejects with git's
   *  own refusal. */
  [Command.vcsSwitchBranch]: (data: {
    cwd: string;
    branch: string;
  }) => string;
  /** Fast-forward from `origin` (never a merge commit); `refusedDirty` with
   *  uncommitted tracked changes, rejects when the branch has diverged. */
  [Command.vcsPull]: (data: { cwd: string }) => PullOutcome;
  /** One commit's message body, per-file stats, and branch. */
  [Command.vcsCommit]: (data: {
    cwd: string;
    sha: string;
  }) => CommitDetail;
  /** Raw unified diff for one path within a commit. */
  [Command.vcsCommitDiff]: (data: {
    cwd: string;
    sha: string;
    path: string;
  }) => string;
  /** The branch-configured remote, falling back to `origin`, as a browser URL. */
  [Command.vcsRemoteUrl]: (data: { cwd: string }) => string | null;
  /** The configured remote's locally-known default branch, when available. */
  [Command.vcsDefaultBranch]: (data: { cwd: string }) => string | null;
  /** Is the `git` CLI installed? Gates the picker's Clone tab. */
  [Command.vcsGitInstalled]: () => boolean;
  /** Whether an SSH private key exists - without one an SSH clone URL can't
   *  authenticate, so the picker offers HTTPS credentials instead. */
  [Command.vcsHasSshKey]: () => boolean;
  /** Can this environment reach the repository (exists + access)? */
  [Command.vcsProbeRemote]: (data: { url: string }) => boolean;
  /** Clone `url` into `root\name`; answers the new project path. Credentials
   *  switch that one clone to HTTPS and are never persisted. */
  [Command.vcsClone]: (data: {
    url: string;
    root: string;
    name: string;
    username?: string;
    password?: string;
  }) => string;
  [Command.vcsWorktreeAdd]: (data: {
    cwd: string;
    branch: string;
    create: boolean;
  }) => string;
  /** Rank prior commits by a natural-language description of the version. */
  [Command.vcsRestoreCandidates]: (data: {
    cwd: string;
    query: string;
    limit?: number;
  }) => RestoreCandidate[];
  /** Non-destructively check the commit out on a `pade/restore-<sha>` branch. */
  [Command.vcsRestoreCheckout]: (data: {
    cwd: string;
    sha: string;
  }) => string;
  /** Launch a task as a tracked runner (the dock) that streams its output. */
  [Command.runnerStart]: (data: {
    id: string;
    command: string;
    cwd?: string | null;
  }) => void;
  [Command.runnerStop]: (data: { id: string }) => void;
  [Command.runnerList]: () => RunnerInfo[];
  [Command.tasksDescriptors]: () => TaskManifestDescriptor[];
  /** Runnable tasks parsed from the project's manifests. */
  [Command.tasksList]: (data: { cwd: string }) => TaskGroup[];
  [Command.usageGet]: (data: { agent: string }) => Usage | null;
  /** Live usage windows for one agent, read from its vendor's own usage
   *  endpoint with the locally-stored token; `null` with no usable signal. */
  [Command.usageAccountAgent]: (data: {
    agent: string;
    forceRefresh: boolean;
  }) => AccountUsage | null;
  /** The free usage-limit reset Claude's account can spend now, or `null`. */
  [Command.limitResetOffer]: () => LimitReset | null;
  /** Spend that reset: clears the account's usage limits. Cannot be undone. */
  [Command.limitResetClaim]: (data: { grantId: string }) => LimitResetOutcome;
  /** The agent CLI's own config files - read, never shadowed. */
  [Command.configList]: (data: { agent: string }) => ConfigFile[];
  /** `rel`: the config file's path relative to the project. */
  [Command.configRead]: (data: { rel: string }) => string;
  /** The Windows Terminal colour schemes PADE can paint its terminal with. */
  [Command.terminalSchemes]: () => TerminalScheme[];
  [Command.launchContext]: () => LaunchContext;
  [Command.settingsGet]: () => Settings;
  /** Add a root folder. `create` makes a missing path first; the outcome says
   *  whether it was added, is missing, or names a file. */
  [Command.workspaceAddRoot]: (data: {
    path: string;
    create: boolean;
  }) => AddRootOutcome;
  [Command.workspaceRemoveRoot]: (data: PathData) => Settings;
  /** Whether a partially-typed path exists (as a dir or a file), plus
   *  child-directory completions for the add-root autocomplete. */
  [Command.workspaceProbePath]: (data: PathData) => PathProbe;
  [Command.workspaceScan]: (data: { root: string }) => ProjectEntry[];
  [Command.workspaceOpen]: (data: PathData) => void;
  /** Delete a consumed auto-handoff doc (`continue-*.md` in the project dir -
   *  the backend refuses anything else). */
  [Command.handoffDocDelete]: (data: {
    dir: string;
    name: string;
  }) => void;
  /** Create a throwaway workspace, mark it owned, and open it; answers its path. */
  [Command.workspaceTemp]: () => string;
  [Command.workspaceMove]: (data: {
    from: string;
    destDir: string;
  }) => string;
  /** Promote a temp workspace into a saved root under a new name. */
  [Command.workspaceRename]: (data: WorkspaceRename) => string;
  /** Rename any workspace's folder where it stands, carrying every agent's
   *  memory of it and PADE's settings along; answers the new path. */
  [Command.workspaceRenameFolder]: (data: Omit<WorkspaceRename, "root">) => string;
  /** Save a temp workspace by copying its files into a fresh saved project;
   *  answers the new dir plus the dependency-install command to run there. */
  [Command.workspaceSaveMigrate]: (data: WorkspaceRename) => SaveMigration;
  /** Set a friendly display label (no disk rename). An `auto` name never
   *  replaces an existing label; a `manual` one always does. */
  [Command.workspaceSetLabel]: (data: {
    path: string;
    name: string;
    source: LabelSource;
  }) => Settings;
  [Command.workspaceDelete]: (data: PathData) => Settings;
  /** Settings with every vanished folder forgotten. */
  [Command.workspacePrune]: () => Settings;
  [Command.workspaceCreate]: (data: {
    root: string;
    name: string;
  }) => string;
  [Command.workspaceClearRecent]: () => Settings;
  [Command.workspaceSetPinned]: (data: {
    path: string;
    pinned: boolean;
  }) => Settings;
  /** Forget a project from the switcher (recents + pins); folder untouched. */
  [Command.workspaceRemoveRecent]: (data: PathData) => Settings;
  /** Persist a drag-reordered pin order (reorders existing pins only). */
  [Command.workspaceSetPinnedOrder]: (data: { paths: string[] }) => Settings;
  /** Delete ANY project directory from disk and forget it. The caller raises a
   *  confirmation and releases the folder first. */
  [Command.workspaceDeleteDirectory]: (data: PathData) => Settings;
  [Command.setDefaultAgent]: (data: { agent: string }) => Settings;
  [Command.setProjectAgent]: (data: {
    path: string;
    agent: string;
  }) => Settings;
  [Command.setPrefs]: (data: { patch: Partial<Prefs> }) => Settings;
}

/** The commands that answer the whole settings - sent through
 *  `sendSettingsMessage` so every reply is adopted into the shared store. */
export type SettingsCommand = {
  [Key in Command]: ReturnType<CommandProtocolMap[Key]> extends Settings ? Key : never
}[Command];

interface EventProtocolMap {
  [EventType.ptyData]: (data: PtyChunk) => void;
  [EventType.ptyExit]: (data: PtyExit) => void;
  /** A session's model or effort may have changed: the agent reported
   *  (`sessionId` is its conversation id), or a settings file every session of
   *  an agent reads changed (`sessionId` null). */
  [EventType.agentModelChanged]: (data: RunningModelChanged) => void;
  [EventType.feedChange]: (data: ChangeEvent) => void;
  /** The ignore rules changed (a `.gitignore` edit, a mid-session `git init`) -
   *  re-ask `feed_ignored` about the events already shown. */
  [EventType.feedIgnoreChanged]: (data: null) => void;
  /** Project MCP config changed in an agent's registered file. */
  [EventType.mcpChanged]: (data: McpChange) => void;
  /** A picker-watched folder gained or lost a child (a project created or
   *  deleted outside PADE). */
  [EventType.directoriesChanged]: (data: null) => void;
  /** A watched project row's tracked files changed - its path, as sent. */
  [EventType.languagesChanged]: (data: string) => void;
  /** The workspace's live git state changed (HEAD, config, `git init`).
   *  Payload-free on purpose: listeners re-fetch what they display. */
  [EventType.gitState]: (data: null) => void;
  [EventType.runnerData]: (data: RunnerData) => void;
  [EventType.runnerExit]: (data: RunnerExit) => void;
  /** The open-windows set/order changed in SOME window (broadcast). */
  [EventType.windowsChanged]: (data: null) => void;
  /** The shared settings changed in SOME window (broadcast) - re-read them
   *  through the one adoption path. */
  [EventType.settingsChanged]: (data: null) => void;
  /** The native OS theme flipped (WebView2's media query can miss it). */
  [EventType.themeChanged]: (data: Scheme) => void;
  /** Native OS drag-and-drop onto the window: Tauri intercepts the webview's
   *  file DnD (the web API never exposes absolute paths) and emits these. */
  [EventType.dragOver]: (data: DragOverPayload) => void;
  [EventType.dragDrop]: (data: DragDropPayload) => void;
  [EventType.dragLeave]: (data: unknown) => void;
}

const COMMAND_RESPONSE_SCHEMAS = {
  [Command.agentsDetect]: z.array(Agent),
  [Command.agentThemePublish]: NO_REPLY,
  [Command.agentModelOptions]: z.array(ModelOption),
  [Command.agentApplyModel]: SwitchOutcome,
  [Command.agentResumeArgs]: z.array(z.string()),
  [Command.agentSessionContext]: SessionContext.nullable(),
  [Command.agentRunningModel]: RunningModel.nullable(),
  [Command.projectAutoname]: z.string().nullable(),
  [Command.sessionGenerateName]: z.string().nullable(),
  [Command.sessionSummarize]: z.string().nullable(),
  [Command.ideDetect]: z.array(Ide),
  [Command.ideSuggest]: z.array(Ide),
  [Command.ideChooseEditor]: Settings,
  [Command.ideKinds]: z.array(EditorKind),
  [Command.ideKindOptions]: z.record(z.string(), z.array(z.string())),
  [Command.ideProjectKinds]: z.record(z.string(), z.string()),
  [Command.ideProjectLanguages]: z.record(z.string(), z.array(LanguageShare)),
  [Command.ideAddEditor]: Settings,
  [Command.ideRemoveEditor]: Settings,
  [Command.ideOpen]: NO_REPLY,
  [Command.ideOpenFile]: NO_REPLY,
  [Command.openInExplorer]: NO_REPLY,
  [Command.revealInExplorer]: NO_REPLY,
  [Command.openInTerminal]: NO_REPLY,
  [Command.openUrl]: NO_REPLY,
  [Command.restart]: NO_REPLY,
  [Command.clipboardImageSave]: z.string().nullable(),
  [Command.recoveryArm]: NO_REPLY,
  [Command.windowCreate]: NO_REPLY,
  [Command.windowRegisterProject]: NO_REPLY,
  [Command.windowFocusProject]: z.boolean(),
  [Command.windowFocusRelative]: z.boolean(),
  [Command.windowList]: z.array(WindowInfo),
  [Command.windowReorder]: NO_REPLY,
  [Command.windowFocusLabel]: z.boolean(),
  [Command.windowTheme]: Scheme,
  [Command.designTools]: z.array(DesignTool),
  [Command.designOpen]: NO_REPLY,
  [Command.discordSetActivity]: NO_REPLY,
  [Command.discordClearActivity]: NO_REPLY,
  [Command.contextMenuStatus]: z.boolean(),
  [Command.contextMenuRegister]: NO_REPLY,
  [Command.contextMenuUnregister]: NO_REPLY,
  [Command.ptySpawn]: NO_REPLY,
  [Command.ptyWrite]: NO_REPLY,
  [Command.ptyResize]: NO_REPLY,
  [Command.ptyKill]: NO_REPLY,
  [Command.ptyList]: z.array(PtySession),
  [Command.ptyHistory]: PtyHistory,
  [Command.sessionTaskStop]: z.boolean(),
  [Command.sessionTaskRunning]: z.boolean(),
  [Command.watchStart]: NO_REPLY,
  [Command.watchStatus]: WatchStatus,
  [Command.watchDirs]: NO_REPLY,
  [Command.watchLanguages]: NO_REPLY,
  [Command.feedDiff]: FeedDiff.nullable(),
  [Command.feedImage]: FeedImage.nullable(),
  [Command.feedVideo]: FeedVideo.nullable(),
  [Command.feedText]: z.string().nullable(),
  [Command.feedIgnored]: z.array(z.string()),
  [Command.membersList]: z.array(WorkspaceMember),
  [Command.vcsStatus]: z.array(StatusEntry),
  [Command.vcsLog]: z.array(Commit),
  [Command.vcsDiff]: z.string(),
  [Command.vcsBranches]: z.array(z.string()),
  [Command.vcsBranchOf]: z.record(z.string(), z.string()),
  [Command.vcsRemoteBranches]: z.array(z.string()),
  [Command.vcsSwitchBranch]: z.string(),
  [Command.vcsPull]: PullOutcome,
  [Command.vcsCommit]: CommitDetail,
  [Command.vcsCommitDiff]: z.string(),
  [Command.vcsRemoteUrl]: z.string().nullable(),
  [Command.vcsDefaultBranch]: z.string().nullable(),
  [Command.vcsGitInstalled]: z.boolean(),
  [Command.vcsHasSshKey]: z.boolean(),
  [Command.vcsProbeRemote]: z.boolean(),
  [Command.vcsClone]: z.string(),
  [Command.vcsWorktreeAdd]: z.string(),
  [Command.vcsRestoreCandidates]: z.array(RestoreCandidate),
  [Command.vcsRestoreCheckout]: z.string(),
  [Command.runnerStart]: NO_REPLY,
  [Command.runnerStop]: NO_REPLY,
  [Command.runnerList]: z.array(RunnerInfo),
  [Command.tasksDescriptors]: z.array(TaskManifestDescriptor),
  [Command.tasksList]: z.array(TaskGroup),
  [Command.usageGet]: Usage.nullable(),
  [Command.usageAccountAgent]: AccountUsage.nullable(),
  [Command.limitResetOffer]: LimitReset.nullable(),
  [Command.limitResetClaim]: LimitResetOutcome,
  [Command.configList]: z.array(ConfigFile),
  [Command.configRead]: z.string(),
  [Command.terminalSchemes]: z.array(TerminalScheme),
  [Command.launchContext]: LaunchContext,
  [Command.settingsGet]: Settings,
  [Command.workspaceAddRoot]: AddRootOutcome,
  [Command.workspaceRemoveRoot]: Settings,
  [Command.workspaceProbePath]: PathProbe,
  [Command.workspaceScan]: z.array(ProjectEntry),
  [Command.workspaceOpen]: NO_REPLY,
  [Command.handoffDocDelete]: NO_REPLY,
  [Command.workspaceTemp]: z.string(),
  [Command.workspaceMove]: z.string(),
  [Command.workspaceRename]: z.string(),
  [Command.workspaceRenameFolder]: z.string(),
  [Command.workspaceSaveMigrate]: SaveMigration,
  [Command.workspaceSetLabel]: Settings,
  [Command.workspaceDelete]: Settings,
  [Command.workspacePrune]: Settings,
  [Command.workspaceCreate]: z.string(),
  [Command.workspaceClearRecent]: Settings,
  [Command.workspaceSetPinned]: Settings,
  [Command.workspaceRemoveRecent]: Settings,
  [Command.workspaceSetPinnedOrder]: Settings,
  [Command.workspaceDeleteDirectory]: Settings,
  [Command.setDefaultAgent]: Settings,
  [Command.setProjectAgent]: Settings,
  [Command.setPrefs]: Settings
} satisfies { [Key in Command]: z.ZodType<ReturnType<CommandProtocolMap[Key]>> };

const EVENT_PAYLOAD_SCHEMAS = {
  [EventType.ptyData]: PtyChunk,
  [EventType.ptyExit]: PtyExit,
  [EventType.agentModelChanged]: RunningModelChanged,
  [EventType.feedChange]: ChangeEvent,
  [EventType.feedIgnoreChanged]: z.null(),
  [EventType.mcpChanged]: McpChange,
  [EventType.directoriesChanged]: z.null(),
  [EventType.languagesChanged]: z.string(),
  [EventType.gitState]: z.null(),
  [EventType.runnerData]: RunnerData,
  [EventType.runnerExit]: RunnerExit,
  [EventType.windowsChanged]: z.null(),
  [EventType.settingsChanged]: z.null(),
  [EventType.themeChanged]: Scheme,
  [EventType.dragOver]: DragOverPayload,
  [EventType.dragDrop]: DragDropPayload,
  [EventType.dragLeave]: z.unknown()
} satisfies { [Key in EventType]: z.ZodType<Parameters<EventProtocolMap[Key]>[0]> };

export const { sendMessage, onMessage } = defineTauriMessaging<CommandProtocolMap, EventProtocolMap>({
  responseSchemas: COMMAND_RESPONSE_SCHEMAS,
  payloadSchemas: EVENT_PAYLOAD_SCHEMAS
});
