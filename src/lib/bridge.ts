// The single boundary between the UI and the Rust core (DRY + SoC).
// Every response is validated with zod, so malformed data fails loudly here
// rather than corrupting the UI. Two helpers own that contract; channels below
// just declare command + schema.

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
  Ide,
  LaunchContext,
  PathProbe,
  ProjectEntry,
  PtyChunk,
  PtyExit,
  PtyHistory,
  PullOutcome,
  RestoreCandidate,
  RunnerData,
  RunnerExit,
  RunnerInfo,
  Settings,
  StatusEntry,
  TaskGroup,
  Usage,
  WindowInfo
} from "@/lib/types";
import type { Prefs } from "@/lib/types";
import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import { readText, writeText } from "@tauri-apps/plugin-clipboard-manager";
import { z } from "zod";

/** Invoke a command and validate its result against `schema`. */
async function call<T>(
  command: string,
  schema: z.ZodType<T>,
  args?: Record<string, unknown>
): Promise<T> {
  return schema.parse(await invoke(command, args));
}

/** Invoke a command that returns nothing meaningful. */
function run(command: string, args?: Record<string, unknown>): Promise<void> {
  return invoke(command, args);
}

/** Subscribe to an event, validating each payload. */
function on<T>(
  event: string,
  schema: z.ZodType<T>,
  callback: (payload: T) => void
): Promise<UnlistenFn> {
  return listen(event, received => callback(schema.parse(received.payload)));
}

/** Detected agent backends. */
export const agents = {
  detect: () => call("agents_detect", z.array(Agent))
};

/** External IDE integration. */
export const ide = {
  detect: () => call("ide_detect", z.array(Ide)),
  /** Installed IDEs ordered for the given project: explicit per-project choice →
   *  rule → fallback → byte-weighted language-coverage ranking. The one source of
   *  the project's editor — `ides[0]` is *the* editor on every surface. */
  suggest: (cwd: string) => call("ide_suggest", z.array(Ide), { cwd }),
  /** Persist an explicit editor pick for the project at `cwd`; it then leads
   *  every `suggest` for that project, so the choice wins on every surface. */
  choose: (args: {
    cwd: string;
    id: string;
  }) => call("ide_choose_editor", Settings, { ...args }),
  /** The project kinds the rules engine shows (label + manifest signals), in the
   *  backend registry's render/priority order — the frontend derives its rows here. */
  kinds: () => call("ide_kinds", z.array(EditorKind)),
  /** Editor ids suited to each project kind (kind → ordered, installed-only), so a
   *  per-kind menu offers only fitting editors (no WebStorm for an Android row). */
  kindOptions: () => call("ide_kind_options", z.record(z.string(), z.array(z.string()))),
  /** Primary detected kind per project path (path → kind id), for the switcher's
   *  per-project language logo. Paths with no recognised markers are omitted. */
  projectKinds: (paths: string[]) => call("ide_project_kinds", z.record(z.string(), z.string()), { paths }),
  /** Add an editor by its executable path. Rejects (throws the message) when the
   *  executable isn't a supported editor; returns the refreshed settings. */
  addEditor: (path: string) => call("ide_add_editor", Settings, { path }),
  /** Remove a user-added editor by its id; returns the refreshed settings. */
  removeEditor: (id: string) => call("ide_remove_editor", Settings, { id }),
  open: (args: {
    command: string;
    path?: string;
    /** 1-based line to jump to (only meaningful when `path` is a file). */
    line?: number;
  }) => run("ide_open", { ...args }),
  /** Open a file inside its `project` workspace window (the folder rides the
   *  launcher CLI for every family with a workspace+file form — never a bare
   *  single-file window), jumping to `line` when given. */
  openFile: (args: {
    command: string;
    project: string;
    file: string;
    line?: number;
  }) => run("ide_open_file", { ...args })
};

/** OS integrations — reveal a project in the file manager or a terminal. */
export const os = {
  explorer: (path: string) => run("open_in_explorer", { path }),
  terminal: (path: string) => run("open_in_terminal", { path }),
  /** Open an http(s) URL in the system's default browser. */
  openUrl: (url: string) => run("open_url", { url })
};

/** System clipboard — read for Ctrl+V paste, write for Ctrl+C copy. */
export const clipboard = {
  readText: () => readText(),
  writeText: (text: string) => writeText(text)
};

/** Multi-window — spawn a fresh app window targeting a project, an empty picker,
 *  or a throwaway workspace. The spawned window routes off its query string. */
export const windows = {
  create: (args: {
    mode: "empty" | "temp" | "open";
    path?: string;
  }) => run("window_create", { ...args }),
  /** Record the project this window now has open (for focus-instead-of-reopen). */
  registerProject: (path: string) => run("window_register_project", { path }),
  /** Focus another window already showing this project; true if one was found. */
  focusProject: (path: string) => call("window_focus_project", z.boolean(), { path }),
  /** Focus the previous/next open PADE window in stable creation order; true if
   *  another window was focused (false when this is the only one). */
  focusRelative: (direction: "previous" | "next") =>
    call("window_focus_relative", z.boolean(), { direction }),
  /** Every open PADE window that has a project, in creation order (= cycle order),
   *  for the switcher's "Open windows" list. */
  list: () => call("window_list", z.array(WindowInfo)),
  /** Focus a specific open window by its label; true if it existed and focused. */
  focus: (label: string) => call("window_focus_label", z.boolean(), { label })
};

/** AI design/UI-generation tools — a roster ranked for the active agent. */
export const design = {
  tools: (agent: string) => call("design_tools", z.array(DesignTool), { agent }),
  /** Open the picked tool in the companion PADE window (native — iframes blocked). */
  open: (url: string) => run("design_open", { url })
};

/** Discord Rich Presence — report PADE ("Playing PADE") on the user's Discord
 *  profile. Best-effort: with Discord closed the backend fails quietly. */
export const discord = {
  setActivity: ({ details, state, image, caption }: {
    details?: string;
    state?: string;
    /** Small overlay art-asset key (the language mark). */
    image?: string;
    /** Hover text for the small overlay. */
    caption?: string;
  }): Promise<void> => run("discord_set_activity", {
    details,
    state,
    image,
    caption
  }),
  clearActivity: (): Promise<void> => run("discord_clear_activity")
};

/** Windows Explorer "Open in PADE" folder context-menu entry. */
export const contextMenu = {
  status: () => call("context_menu_status", z.boolean()),
  register: () => run("context_menu_register"),
  unregister: () => run("context_menu_unregister")
};

/** Terminal / PTY channel. Sessions are addressed by `id`; callbacks receive it
 *  so a listener can route to the right terminal. */
export const pty = {
  spawn: (args: {
    id: string;
    command: string | null;
    cwd: string | null;
    cols: number;
    rows: number;
    /** Extra args for `command` — e.g. the project path for a terminal editor. */
    args?: string[];
  }) =>
    run("pty_spawn", { ...args }),
  write: (args: {
    id: string;
    data: string;
  }) => run("pty_write", { ...args }),
  resize: (args: {
    id: string;
    cols: number;
    rows: number;
  }) => run("pty_resize", { ...args }),
  kill: (id: string) => run("pty_kill", { id }),
  /** Everything a terminal needs to paint a session it is attaching to mid-flight
   *  (a remounted component, a reloaded window), and the sequence number of the last
   *  chunk already inside it — chunks above that one arrived live and still need
   *  writing. A PTY has no scrollback of its own, so without this the terminal has
   *  nothing to draw and sits blank while the agent waits, quite happily, for input. */
  history: (id: string) => call("pty_history", PtyHistory, { id }),
  /** Ask the namer for a concise session name from its transcript (null until
   *  there's enough conversation, or if nothing usable is produced). */
  generateName: (args: {
    id: string;
    agent: string;
  }) => call("session_generate_name", z.string().nullable(), { ...args }),
  onData: (callback: (chunk: PtyChunk) => void) => on("pty://data", PtyChunk, callback),
  onExit: (callback: (id: string) => void) =>
    on("pty://exit", PtyExit, payload => callback(payload.id))
};

/** Change Feed / filesystem watcher channel. */
export const feed = {
  /** Watch `path` — the open workspace's root — so the feed follows the project
   *  on screen, not the process's cwd. Idempotent per root; a call for a new root
   *  re-roots the watcher (drops the old one, clears its per-file bookkeeping). */
  start: (path: string) => run("watch_start", { root: path }),
  /** The card's git-free preview for a path: the watch session's first-touch
   *  baseline vs the file's current content (`null` when nothing was snapshotted
   *  — binary, too large, or a path with no captured baseline). The frontend
   *  renders the unified diff, so untracked/ignored files preview like any other. */
  diff: ({ path }: {
    path: string;
  }) => call("feed_diff", FeedDiff.nullable(), { path }),
  onChange: (callback: (event: ChangeEvent) => void) =>
    on("feed://change", ChangeEvent, callback)
};

/** The picker's folder watcher: hand it the parents of the rows on the page and
 *  it reports whenever one of them gains or loses a child — a project created or
 *  deleted outside PADE — so the list can catch up on its own. */
export const dirs = {
  watch: (paths: string[]) => run("watch_dirs", { dirs: paths }),
  onChange: (callback: () => void) => on("dirs://changed", z.null(), () => callback())
};

/** Native OS drag-and-drop onto the window. Tauri intercepts the webview's
 *  file DnD (the web API never exposes absolute paths) and emits these events
 *  instead — how a folder dragged from Explorer or an IDE reaches the app. */
export const dragDrop = {
  onOver: (callback: (payload: DragOverPayload) => void) => on("tauri://drag-over", DragOverPayload, callback),
  onDrop: (callback: (payload: DragDropPayload) => void) => on("tauri://drag-drop", DragDropPayload, callback),
  onLeave: (callback: () => void) => on("tauri://drag-leave", z.unknown(), () => callback())
};

/** Version-control review channel. */
export const vcs = {
  status: () => call("vcs_status", z.array(StatusEntry)),
  log: (limit = 20) => call("vcs_log", z.array(Commit), { limit }),
  diff: ({ path, staged = false }: {
    path: string;
    staged?: boolean;
  }) =>
    call("vcs_diff", z.string(), {
      path,
      staged
    }),
  branches: () => call("vcs_branches", z.array(z.string())),
  /** Current HEAD branch per project path (path → branch), for the switcher's
   *  per-project branch chip. Non-repo / detached paths are omitted. */
  branchOf: (paths: string[]) => call("vcs_branch_of", z.record(z.string(), z.string()), { paths }),
  /** Fast-forward the open workspace from `origin` (never a merge commit).
   *  Resolves `refusedDirty` when the tree has uncommitted tracked changes;
   *  throws git's message when the branch has diverged (no fast-forward). */
  pull: () => call("vcs_pull", PullOutcome),
  /** One commit's message body, per-file stats, and branch. */
  commit: (sha: string) => call("vcs_commit", CommitDetail, { sha }),
  /** Raw unified diff for one path within a commit. */
  commitDiff: ({ sha, path }: {
    sha: string;
    path: string;
  }) => call("vcs_commit_diff", z.string(), {
    sha,
    path
  }),
  /** The `origin` remote as a browsable `https://host/owner/repo` URL, or null. */
  remoteUrl: () => call("vcs_remote_url", z.string().nullable()),
  /** Is the `git` CLI installed? Gates the picker's Clone tab. */
  gitInstalled: () => call("vcs_git_installed", z.boolean()),
  /** Whether an SSH private key exists — without one an `ssh://`/`git@` clone
   *  URL can't authenticate, so the picker offers HTTPS credentials instead. */
  hasSshKey: () => call("vcs_has_ssh_key", z.boolean()),
  /** Can the current environment reach this repository (exists + access)?
   *  Live check behind the Clone tab's URL field. */
  probeRemote: (url: string) => call("vcs_probe_remote", z.boolean(), { url }),
  /** Clone `url` into `root\name`; returns the new project path. Credentials
   *  switch the clone to HTTPS for that one command and are never persisted. */
  clone: (args: {
    url: string;
    root: string;
    name: string;
    username?: string;
    password?: string;
  }) => call("vcs_clone", z.string(), { ...args }),
  worktreeAdd: (args: {
    branch: string;
    create: boolean;
  }) =>
    call("vcs_worktree_add", z.string(), { ...args }),
  /** Rank prior commits by a natural-language description of the version to restore. */
  restoreCandidates: (args: {
    query: string;
    limit?: number;
  }) =>
    call("vcs_restore_candidates", z.array(RestoreCandidate), { ...args }),
  /** Non-destructively check the chosen commit out on a `pade/restore-<sha>` branch. */
  restoreCheckout: (sha: string) => call("vcs_restore_checkout", z.string(), { sha })
};

/** Task-runner dock — a task launched as a tracked runner that streams its output.
 *  Piping a runner's output into an agent is done in the UI via `pty.write`. */
export const runner = {
  start: (args: {
    id: string;
    command: string;
    cwd?: string | null;
  }) =>
    run("runner_start", { ...args }),
  stop: (id: string) => run("runner_stop", { id }),
  list: () => call("runner_list", z.array(RunnerInfo)),
  onData: (callback: (payload: RunnerData) => void) =>
    on("runner://data", RunnerData, callback),
  onExit: (callback: (payload: RunnerExit) => void) =>
    on("runner://exit", RunnerExit, callback)
};

/** Task runner channel — runnable tasks parsed from project manifests. */
export const tasks = {
  list: () => call("tasks_list", z.array(TaskGroup))
};

/** Agent usage / quota channel — never spends message quota: local data plus a
 *  cached call to the vendor's OAuth usage endpoint for the live account
 *  windows. */
export const usage = {
  get: (agent: string) => call("usage_get", Usage.nullable(), { agent }),
  /** Live claude.ai usage windows (5-hour session + 7-day weekly) via the OAuth
   *  endpoint — the same numbers `claude /usage` shows. */
  account: () => call("usage_account", AccountUsage.nullable())
};

/** Agent config channel — reads the CLI's own config files, never shadows them. */
export const config = {
  list: (agent: string) => call("config_list", z.array(ConfigFile), { agent }),
  read: (rel: string) => call("config_read", z.string(), { rel })
};

/** Workspace & projects channel. */
export const workspace = {
  context: () => call("launch_context", LaunchContext),
  settings: () => call("settings_get", Settings),
  /** Add a root folder. `create` asks the backend to `create_dir_all` a missing
   *  path before adding it; the discriminated outcome says whether it was added,
   *  is missing (so the caller can offer to create it), or names a file. */
  addRoot: (args: {
    path: string;
    create: boolean;
  }) => call("workspace_add_root", AddRootOutcome, { ...args }),
  removeRoot: (path: string) => call("workspace_remove_root", Settings, { path }),
  /** Probe a partially-typed root path: whether it already exists (as a dir or a
   *  file), plus child-directory completions for the add-root autocomplete. */
  probePath: (path: string) => call("workspace_probe_path", PathProbe, { path }),
  scan: (root: string) => call("workspace_scan", z.array(ProjectEntry), { root }),
  open: (path: string) => run("workspace_open", { path }),
  /** Delete a consumed auto-handoff doc (`continue-*.md` in the project dir —
   *  the backend refuses anything else). */
  deleteHandoffDoc: (args: {
    dir: string;
    name: string;
  }) => run("handoff_doc_delete", { ...args }),
  /** Create a throwaway workspace, mark it owned, and open it — chdirs the
   *  process and records it in Recents. Returns its path. */
  temp: () => call("workspace_temp", z.string()),
  move: (args: {
    from: string;
    destDir: string;
  }) => call("workspace_move", z.string(), { ...args }),
  rename: (args: {
    from: string;
    newName: string;
  }) => call("workspace_rename", z.string(), { ...args }),
  /** Set a friendly display label (no disk rename). */
  setLabel: (args: {
    path: string;
    name: string;
  }) => call("workspace_set_label", Settings, { ...args }),
  /** Suggest a name for a temp workspace via the agent CLI, else a heuristic. */
  autoname: (args: {
    path: string;
    agent: string;
  }) => call("project_autoname", z.string().nullable(), { ...args }),
  delete: (path: string) => call("workspace_delete", Settings, { path }),
  /** Settings with every vanished folder forgotten (see `dirs`). */
  prune: () => call("workspace_prune", Settings),
  create: (args: {
    root: string;
    name: string;
  }) => call("workspace_create", z.string(), { ...args }),
  clearRecent: () => call("workspace_clear_recent", Settings),
  /** Pin or unpin a project in the switcher; returns the refreshed settings. */
  setPinned: (args: {
    path: string;
    pinned: boolean;
  }) => call("workspace_set_pinned", Settings, { ...args }),
  /** Forget a project from the switcher (recents + pins); folder untouched. */
  removeRecent: (path: string) => call("workspace_remove_recent", Settings, { path }),
  /** Persist a drag-reordered pin order (reorders existing pins only). */
  setPinnedOrder: (paths: string[]) => call("workspace_set_pinned_order", Settings, { paths }),
  /** Delete ANY project directory from disk and forget it — the switcher's "Delete
   *  directory". The caller raises a confirmation and releases the folder first. */
  deleteDirectory: (path: string) => call("workspace_delete_directory", Settings, { path }),
  setDefaultAgent: (agent: string) => call("set_default_agent", Settings, { agent }),
  setProjectAgent: (args: {
    path: string;
    agent: string;
  }) =>
    call("set_project_agent", Settings, { ...args }),
  setPrefs: (prefs: Prefs) => call("set_prefs", Settings, { prefs })
};
