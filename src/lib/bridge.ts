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
  EditorKind,
  Ide,
  LaunchContext,
  PathProbe,
  ProjectEntry,
  PtyChunk,
  PtyExit,
  PtyHistory,
  RestoreCandidate,
  RunnerData,
  RunnerExit,
  RunnerInfo,
  SessionUsage,
  Settings,
  StatusEntry,
  TaskGroup,
  Usage
} from "@/lib/types";
import type { Prefs } from "@/lib/types";
import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
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
  /** Installed IDEs ordered for the current project: rule → fallback → auto-rank;
   *  a multi-kind project (monorepo) puts generalists before specialists. */
  suggest: () => call("ide_suggest", z.array(Ide)),
  /** The project kinds the rules engine shows (label + manifest signals), in the
   *  backend registry's render/priority order — the frontend derives its rows here. */
  kinds: () => call("ide_kinds", z.array(EditorKind)),
  /** Editor ids suited to each project kind (kind → ordered, installed-only), so a
   *  per-kind menu offers only fitting editors (no WebStorm for an Android row). */
  kindOptions: () => call("ide_kind_options", z.record(z.string(), z.array(z.string()))),
  /** Add an editor by its executable path. Rejects (throws the message) when the
   *  executable isn't a supported editor; returns the refreshed settings. */
  addEditor: (path: string) => call("ide_add_editor", Settings, { path }),
  /** Primary detected project kind of the current dir (e.g. "web"), or null. */
  projectKind: () => call("ide_project_kind", z.string().nullable()),
  open: (args: {
    command: string;
    path?: string;
    /** 1-based line to jump to (only meaningful when `path` is a file). */
    line?: number;
  }) => run("ide_open", { ...args }),
  /** Open a file in the window that already has `project` open (JetBrains via its
   *  URL scheme, others via the CLI), jumping to `line` when given. */
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
  focusProject: (path: string) => call("window_focus_project", z.boolean(), { path })
};

/** AI design/UI-generation tools — a roster ranked for the active agent. */
export const design = {
  tools: (agent: string) => call("design_tools", z.array(DesignTool), { agent }),
  /** Open the picked tool in the companion PADE window (native — iframes blocked). */
  open: (url: string) => run("design_open", { url })
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
  /** The session's rolling, ANSI-stripped transcript tail. */
  transcript: (id: string) => call("session_transcript", z.string(), { id }),
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
  start: () => run("watch_start"),
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
  /** The current HEAD branch name, or null on a detached HEAD / non-repo. */
  currentBranch: () => call("vcs_current_branch", z.string().nullable()),
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

/** Agent usage / quota channel — never spends message quota. `session` reads
 *  local data only; `get`/`account` also make a cached call to the vendor's
 *  OAuth usage endpoint for the live account windows. */
export const usage = {
  get: (agent: string) => call("usage_get", Usage.nullable(), { agent }),
  /** The active session's context-window state for the latest session in `cwd`. */
  session: (cwd: string) => call("usage_session", SessionUsage.nullable(), { cwd }),
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
  clone: (args: {
    root: string;
    url: string;
  }) => call("workspace_clone", z.string(), { ...args }),
  clearRecent: () => call("workspace_clear_recent", Settings),
  setDefaultAgent: (agent: string) => call("set_default_agent", Settings, { agent }),
  setProjectAgent: (args: {
    path: string;
    agent: string;
  }) =>
    call("set_project_agent", Settings, { ...args }),
  setPrefs: (prefs: Prefs) => call("set_prefs", Settings, { prefs })
};
