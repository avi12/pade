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
  McpChange,
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
  Settings,
  StatusEntry,
  TaskGroup,
  Usage,
  WindowInfo,
  WorkspaceMember
} from "@/lib/types";
import type { Prefs, Scheme } from "@/lib/types";
import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { readText, writeText } from "@tauri-apps/plugin-clipboard-manager";
import { z } from "zod";

/** Invoke a command and validate its result against `schema`. */
interface CommandRequest {
  args?: Record<string, unknown>;
  command: string;
}

interface CommandCall<T> extends CommandRequest {
  schema: z.ZodType<T>;
}

async function call<T>({ command, schema, args }: CommandCall<T>): Promise<T> {
  return schema.parse(await invoke(command, args));
}

/** Invoke a command that returns nothing meaningful. */
type CommandRun = CommandRequest;

function run({ command, args }: CommandRun): Promise<void> {
  return invoke(command, args);
}

/** Subscribe to an event, validating each payload. Scoped to this window's own
 *  label so a sibling window's targeted emit (the backend routes each window's
 *  file-watch changes back to it with `emit_to`) never leaks into this feed;
 *  an app-wide `emit` broadcast still reaches every window's scoped listener. */
interface EventListener<T> {
  callback: (payload: T) => void;
  event: string;
  schema: z.ZodType<T>;
}

function on<T>({ event, schema, callback }: EventListener<T>): Promise<UnlistenFn> {
  return listen(event, received => callback(schema.parse(received.payload)), {
    target: getCurrentWindow().label
  });
}

/** Detected agent backends. */
export const agents = {
  detect: () => call({
    command: "agents_detect",
    schema: z.array(Agent)
  }),
  /** Force every installed agent's own theme config in `workspace` to ADE's
   *  scheme (Claude re-reads its settings live — a flip re-themes a running
   *  session). The terminal protocol can't carry this through ConPTY. */
  syncTheme: (args: {
    workspace: string;
    scheme: Scheme;
  }) => run({
    command: "theme_sync",
    args: {
      ...args
    }
  })
};

/** External IDE integration. */
export const ide = {
  detect: () => call({
    command: "ide_detect",
    schema: z.array(Ide)
  }),
  /** Installed IDEs ordered for the given project: explicit per-project choice →
   *  rule → fallback → byte-weighted language-coverage ranking. The one source of
   *  the project's editor — `ides[0]` is *the* editor on every surface. */
  suggest: (cwd: string) => call({
    command: "ide_suggest",
    schema: z.array(Ide),
    args: {
      cwd
    }
  }),
  /** Persist an explicit editor pick for the project at `cwd`; it then leads
   *  every `suggest` for that project, so the choice wins on every surface. */
  choose: (args: {
    cwd: string;
    id: string;
  }) => call({
    command: "ide_choose_editor",
    schema: Settings,
    args: {
      ...args
    }
  }),
  /** The project kinds the rules engine shows (label + manifest signals), in the
   *  backend registry's render/priority order — the frontend derives its rows here. */
  kinds: () => call({
    command: "ide_kinds",
    schema: z.array(EditorKind)
  }),
  /** Editor ids suited to each project kind (kind → ordered, installed-only), so a
   *  per-kind menu offers only fitting editors (no WebStorm for an Android row). */
  kindOptions: () => call({
    command: "ide_kind_options",
    schema: z.record(z.string(), z.array(z.string()))
  }),
  /** Primary detected kind per project path (path → kind id), for the switcher's
   *  per-project language logo. Paths with no recognised markers are omitted. */
  projectKinds: (paths: string[]) => call({
    command: "ide_project_kinds",
    schema: z.record(z.string(), z.string()),
    args: {
      paths
    }
  }),
  /** Add an editor by its executable path. Rejects (throws the message) when the
   *  executable isn't a supported editor; returns the refreshed settings. */
  addEditor: (path: string) => call({
    command: "ide_add_editor",
    schema: Settings,
    args: {
      path
    }
  }),
  /** Remove a user-added editor by its id; returns the refreshed settings. */
  removeEditor: (id: string) => call({
    command: "ide_remove_editor",
    schema: Settings,
    args: {
      id
    }
  }),
  open: (args: {
    command: string;
    path?: string;
    /** 1-based line to jump to (only meaningful when `path` is a file). */
    line?: number;
  }) => run({
    command: "ide_open",
    args: {
      ...args
    }
  }),
  /** Open a file inside its `project` workspace window (the folder rides the
   *  launcher CLI for every family with a workspace+file form — never a bare
   *  single-file window), jumping to `line` when given. */
  openFile: (args: {
    command: string;
    project: string;
    file: string;
    line?: number;
  }) => run({
    command: "ide_open_file",
    args: {
      ...args
    }
  })
};

/** OS integrations — reveal a project in the file manager or a terminal. */
export const os = {
  explorer: (path: string) => run({
    command: "open_in_explorer",
    args: {
      path
    }
  }),
  terminal: (path: string) => run({
    command: "open_in_terminal",
    args: {
      path
    }
  }),
  /** Open an http(s) URL in the system's default browser. */
  openUrl: (url: string) => run({
    command: "open_url",
    args: {
      url
    }
  })
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
  }) => run({
    command: "window_create",
    args: {
      ...args
    }
  }),
  /** Set this window's OS title (title bar + taskbar) — the one place the UI
   *  drives Tauri's window title, so no surface recomputes it independently. */
  setTitle: (title: string) => getCurrentWindow().setTitle(title),
  /** Record the project this window now has open (for focus-instead-of-reopen). */
  registerProject: (path: string) => run({
    command: "window_register_project",
    args: {
      path
    }
  }),
  /** Focus another window already showing this project; true if one was found. */
  focusProject: (path: string) => call({
    command: "window_focus_project",
    schema: z.boolean(),
    args: {
      path
    }
  }),
  /** Focus the previous/next open PADE window in stable creation order; true if
   *  another window was focused (false when this is the only one). */
  focusRelative: (direction: "previous" | "next") =>
    call({
      command: "window_focus_relative",
      schema: z.boolean(),
      args: {
        direction
      }
    }),
  /** Every open PADE window that has a project, in the user's explicit order
   *  (= cycle order), for the switcher's "Open windows" list. */
  list: () => call({
    command: "window_list",
    schema: z.array(WindowInfo)
  }),
  /** Persist the drag-reordered "Open windows" order (session-scoped by label).
   *  The one source both this list and the Ctrl+Alt+[ / ] cycle read. */
  reorder: (labels: string[]) => run({
    command: "window_reorder",
    args: {
      labels
    }
  }),
  /** Focus a specific open window by its label; true if it existed and focused. */
  focus: (label: string) => call({
    command: "window_focus_label",
    schema: z.boolean(),
    args: {
      label
    }
  }),
  /** Intercept this window's close (the title-bar X): the handler runs to
   *  completion first — the graceful-leave hook — and only then is the window
   *  destroyed (Tauri's own onCloseRequested contract when nothing prevents).
   *  Returns the unlisten function. */
  onCloseRequested: (handler: () => Promise<void>) =>
    getCurrentWindow().onCloseRequested(handler)
};

/** AI design/UI-generation tools — a roster ranked for the active agent. */
export const design = {
  tools: (agent: string) => call({
    command: "design_tools",
    schema: z.array(DesignTool),
    args: {
      agent
    }
  }),
  /** Open the picked tool in the companion PADE window (native — iframes blocked). */
  open: (url: string) => run({
    command: "design_open",
    args: {
      url
    }
  })
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
  }): Promise<void> => run({
    command: "discord_set_activity",
    args: {
      details,
      state,
      image,
      caption
    }
  }),
  clearActivity: (): Promise<void> => run({ command: "discord_clear_activity" })
};

/** Windows Explorer "Open in PADE" folder context-menu entry. */
export const contextMenu = {
  status: () => call({
    command: "context_menu_status",
    schema: z.boolean()
  }),
  register: () => run({ command: "context_menu_register" }),
  unregister: () => run({ command: "context_menu_unregister" })
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
    /** ADE's resolved appearance at spawn — env-themed CLIs start matching it. */
    scheme?: Scheme;
    /** Stable conversation id to pin/resume the session to (`claude
     *  --session-id`), so a restart lands back in THIS conversation. */
    conversationId?: string;
  }) =>
    run({
      command: "pty_spawn",
      args: {
        ...args
      }
    }),
  write: (args: {
    id: string;
    data: string;
  }) => run({
    command: "pty_write",
    args: {
      ...args
    }
  }),
  resize: (args: {
    id: string;
    cols: number;
    rows: number;
  }) => run({
    command: "pty_resize",
    args: {
      ...args
    }
  }),
  kill: (id: string) => run({
    command: "pty_kill",
    args: {
      id
    }
  }),
  /** Every live session the backend still hosts — the roster a reloaded window
   *  intersects its persisted pane mapping with to re-attach (see
   *  `session-restore`) instead of stranding running agents invisibly. */
  list: () => call({
    command: "pty_list",
    schema: z.array(PtySession)
  }),
  /** Everything a terminal needs to paint a session it is attaching to mid-flight
   *  (a remounted component, a reloaded window), and the sequence number of the last
   *  chunk already inside it — chunks above that one arrived live and still need
   *  writing. A PTY has no scrollback of its own, so without this the terminal has
   *  nothing to draw and sits blank while the agent waits, quite happily, for input. */
  history: (id: string) => call({
    command: "pty_history",
    schema: PtyHistory,
    args: {
      id
    }
  }),
  /** Ask the namer for a concise session name from its transcript (null until
   *  there's enough conversation, or if nothing usable is produced). */
  generateName: (args: {
    id: string;
    agent: string;
  }) => call({
    command: "session_generate_name",
    schema: z.string().nullable(),
    args: {
      ...args
    }
  }),
  onData: (callback: (chunk: PtyChunk) => void) => on({
    event: "pty://data",
    schema: PtyChunk,
    callback
  }),
  onExit: (callback: (id: string) => void) =>
    on({
      event: "pty://exit",
      schema: PtyExit,
      callback: payload => callback(payload.id)
    })
};

/** A Change Feed image preview: the file's bytes as a ready-to-use `data:` URL,
 *  so the card renders it with a plain `<img src>`. The whole payload is nullable
 *  at the boundary (see `feed.image`). Declared here — the only place it's read —
 *  rather than in the shared types module. */
const FeedImage = z.object({
  dataUrl: z.string()
});

/** Change Feed / filesystem watcher channel. */
export const feed = {
  /** Watch `path` — the open workspace's root — so the feed follows the project
   *  on screen, not the process's cwd. Idempotent per root; a call for a new root
   *  re-roots the watcher (drops the old one, clears its per-file bookkeeping). */
  start: (path: string) => run({
    command: "watch_start",
    args: {
      root: path
    }
  }),
  /** The card's git-free preview for a path: the watch session's first-touch
   *  baseline vs the file's current content (`null` when nothing was snapshotted
   *  — binary, too large, or a path with no captured baseline). The frontend
   *  renders the unified diff, so untracked/ignored files preview like any other. */
  diff: ({ path }: {
    path: string;
  }) => call({
    command: "feed_diff",
    schema: FeedDiff.nullable(),
    args: {
      path
    }
  }),
  /** The rendered image preview for an image path: the file's current bytes as a
   *  ready-to-use `data:` URL for a plain `<img src>` (no asset protocol). `null`
   *  when the path isn't a previewable image, is gone, or is over the backend's
   *  size cap — the card then falls back to its text summary. SVG rides the same
   *  data-URL path, so its markup is never inlined into the DOM. */
  image: ({ path }: {
    path: string;
  }) => call({
    command: "feed_image",
    schema: FeedImage.nullable(),
    args: {
      path
    }
  }),
  /** The current text of a watched markdown/HTML path, for the card's Preview
   *  toggle (the file rendered as it is now). `null` when the path wasn't
   *  snapshotted this session, is gone, or is binary / over the backend's size
   *  cap — the card then keeps its diff and offers no preview. */
  text: ({ path }: {
    path: string;
  }) => call({
    command: "feed_text",
    schema: z.string().nullable(),
    args: {
      path
    }
  }),
  onChange: (callback: (event: ChangeEvent) => void) =>
    on({
      event: "feed://change",
      schema: ChangeEvent,
      callback
    }),
  /** The ignore rules changed (a `.gitignore` edited/created/deleted, or a
   *  mid-session `git init` flipped the policy) — re-ask `ignored` about the
   *  events already shown. */
  onIgnoreChanged: (callback: () => void) => on({
    event: "feed://ignore-changed",
    schema: z.null(),
    callback: () => callback()
  }),
  /** The subset of `paths` the current ignore policy excludes. */
  ignored: (paths: string[]) => call({
    command: "feed_ignored",
    schema: z.array(z.string()),
    args: {
      paths
    }
  })
};

/** MCP config changes. A running agent only picks up an added/removed MCP
 *  server by restarting (there's no in-session reload), so ADE watches the
 *  project's config file (`.mcp.json` for Claude) and restarts the affected
 *  sessions when the set of servers changes — not on a value-only edit. */
export const mcp = {
  onChanged: (callback: (change: McpChange) => void) => on({
    event: "mcp://changed",
    schema: McpChange,
    callback
  })
};

/** Manifest-driven workspace members — the Change Feed's grouping ground
 *  truth: one backend census walk confirms which directories are real packages
 *  and the root's workspace-defining files say which of them are members. */
export const members = {
  /** Confirmed members of the workspace at `root`; the root itself is first. */
  list: (root: string) => call({
    command: "members_list",
    schema: z.array(WorkspaceMember),
    args: {
      root
    }
  })
};

/** The picker's folder watcher: hand it the parents of the rows on the page and
 *  it reports whenever one of them gains or loses a child — a project created or
 *  deleted outside PADE — so the list can catch up on its own. */
export const dirs = {
  watch: (paths: string[]) => run({
    command: "watch_dirs",
    args: {
      dirs: paths
    }
  }),
  onChange: (callback: () => void) => on({
    event: "dirs://changed",
    schema: z.null(),
    callback: () => callback()
  })
};

/** Native OS drag-and-drop onto the window. Tauri intercepts the webview's
 *  file DnD (the web API never exposes absolute paths) and emits these events
 *  instead — how a folder dragged from Explorer or an IDE reaches the app. */
export const dragDrop = {
  onOver: (callback: (payload: DragOverPayload) => void) => on({
    event: "tauri://drag-over",
    schema: DragOverPayload,
    callback
  }),
  onDrop: (callback: (payload: DragDropPayload) => void) => on({
    event: "tauri://drag-drop",
    schema: DragDropPayload,
    callback
  }),
  onLeave: (callback: () => void) => on({
    event: "tauri://drag-leave",
    schema: z.unknown(),
    callback: () => callback()
  })
};

/** Version-control review channel. */
export const vcs = {
  status: () => call({
    command: "vcs_status",
    schema: z.array(StatusEntry)
  }),
  log: (limit = 20) => call({
    command: "vcs_log",
    schema: z.array(Commit),
    args: {
      limit
    }
  }),
  diff: ({ path, staged = false }: {
    path: string;
    staged?: boolean;
  }) =>
    call({
      command: "vcs_diff",
      schema: z.string(),
      args: {
        path,
        staged
      }
    }),
  branches: () => call({
    command: "vcs_branches",
    schema: z.array(z.string())
  }),
  /** Current HEAD branch per project path (path → branch), for the switcher's
   *  per-project branch chip. Non-repo / detached paths are omitted. */
  branchOf: (paths: string[]) => call({
    command: "vcs_branch_of",
    schema: z.record(z.string(), z.string()),
    args: {
      paths
    }
  }),
  /** Fast-forward the open workspace from `origin` (never a merge commit).
   *  Resolves `refusedDirty` when the tree has uncommitted tracked changes;
   *  throws git's message when the branch has diverged (no fast-forward). */
  pull: () => call({
    command: "vcs_pull",
    schema: PullOutcome
  }),
  /** One commit's message body, per-file stats, and branch. */
  commit: (sha: string) => call({
    command: "vcs_commit",
    schema: CommitDetail,
    args: {
      sha
    }
  }),
  /** Raw unified diff for one path within a commit. */
  commitDiff: ({ sha, path }: {
    sha: string;
    path: string;
  }) => call({
    command: "vcs_commit_diff",
    schema: z.string(),
    args: {
      sha,
      path
    }
  }),
  /** The `origin` remote as a browsable `https://host/owner/repo` URL, or null. */
  remoteUrl: () => call({
    command: "vcs_remote_url",
    schema: z.string().nullable()
  }),
  /** Is the `git` CLI installed? Gates the picker's Clone tab. */
  gitInstalled: () => call({
    command: "vcs_git_installed",
    schema: z.boolean()
  }),
  /** Whether an SSH private key exists — without one an `ssh://`/`git@` clone
   *  URL can't authenticate, so the picker offers HTTPS credentials instead. */
  hasSshKey: () => call({
    command: "vcs_has_ssh_key",
    schema: z.boolean()
  }),
  /** Can the current environment reach this repository (exists + access)?
   *  Live check behind the Clone tab's URL field. */
  probeRemote: (url: string) => call({
    command: "vcs_probe_remote",
    schema: z.boolean(),
    args: {
      url
    }
  }),
  /** Clone `url` into `root\name`; returns the new project path. Credentials
   *  switch the clone to HTTPS for that one command and are never persisted. */
  clone: (args: {
    url: string;
    root: string;
    name: string;
    username?: string;
    password?: string;
  }) => call({
    command: "vcs_clone",
    schema: z.string(),
    args: {
      ...args
    }
  }),
  worktreeAdd: (args: {
    branch: string;
    create: boolean;
  }) =>
    call({
      command: "vcs_worktree_add",
      schema: z.string(),
      args: {
        ...args
      }
    }),
  /** Rank prior commits by a natural-language description of the version to restore. */
  restoreCandidates: (args: {
    query: string;
    limit?: number;
  }) =>
    call({
      command: "vcs_restore_candidates",
      schema: z.array(RestoreCandidate),
      args: {
        ...args
      }
    }),
  /** Non-destructively check the chosen commit out on a `pade/restore-<sha>` branch. */
  restoreCheckout: (sha: string) => call({
    command: "vcs_restore_checkout",
    schema: z.string(),
    args: {
      sha
    }
  }),
  /** Fires when the workspace's live git state changes — a branch switch (HEAD),
   *  a remote added/removed (config), or `git init` creating the repo. Carries no
   *  payload on purpose: listeners re-fetch whatever git state they display, so
   *  the event can never hand them stale or partial data. */
  onStateChanged: (callback: () => void) => on({
    event: "git://state",
    schema: z.null(),
    callback: () => callback()
  })
};

/** Task-runner dock — a task launched as a tracked runner that streams its output.
 *  Piping a runner's output into an agent is done in the UI via `pty.write`. */
export const runner = {
  start: (args: {
    id: string;
    command: string;
    cwd?: string | null;
  }) =>
    run({
      command: "runner_start",
      args: {
        ...args
      }
    }),
  stop: (id: string) => run({
    command: "runner_stop",
    args: {
      id
    }
  }),
  list: () => call({
    command: "runner_list",
    schema: z.array(RunnerInfo)
  }),
  onData: (callback: (payload: RunnerData) => void) =>
    on({
      event: "runner://data",
      schema: RunnerData,
      callback
    }),
  onExit: (callback: (payload: RunnerExit) => void) =>
    on({
      event: "runner://exit",
      schema: RunnerExit,
      callback
    })
};

/** Task runner channel — runnable tasks parsed from project manifests. */
export const tasks = {
  list: () => call({
    command: "tasks_list",
    schema: z.array(TaskGroup)
  })
};

/** Agent usage / quota channel — never spends message quota: local data plus a
 *  cached call to the vendor's OAuth usage endpoint for the live account
 *  windows. */
export const usage = {
  get: (agent: string) => call({
    command: "usage_get",
    schema: Usage.nullable(),
    args: {
      agent
    }
  }),
  /** Live claude.ai usage windows (5-hour session + 7-day weekly) via the OAuth
   *  endpoint — the same numbers `claude /usage` shows. */
  account: () => call({
    command: "usage_account",
    schema: AccountUsage.nullable()
  }),
  /** Live usage windows for a specific agent (`claude`, `codex`) — each read from
   *  that vendor's own usage endpoint with its locally-stored token. `null` for an
   *  agent with no usable local usage signal. */
  accountFor: (agent: string) => call({
    command: "usage_account_agent",
    schema: AccountUsage.nullable(),
    args: {
      agent
    }
  })
};

/** Agent config channel — reads the CLI's own config files, never shadows them. */
export const config = {
  list: (agent: string) => call({
    command: "config_list",
    schema: z.array(ConfigFile),
    args: {
      agent
    }
  }),
  read: (rel: string) => call({
    command: "config_read",
    schema: z.string(),
    args: {
      rel
    }
  })
};

/** Workspace & projects channel. */
export const workspace = {
  context: () => call({
    command: "launch_context",
    schema: LaunchContext
  }),
  settings: () => call({
    command: "settings_get",
    schema: Settings
  }),
  /** Add a root folder. `create` asks the backend to `create_dir_all` a missing
   *  path before adding it; the discriminated outcome says whether it was added,
   *  is missing (so the caller can offer to create it), or names a file. */
  addRoot: (args: {
    path: string;
    create: boolean;
  }) => call({
    command: "workspace_add_root",
    schema: AddRootOutcome,
    args: {
      ...args
    }
  }),
  removeRoot: (path: string) => call({
    command: "workspace_remove_root",
    schema: Settings,
    args: {
      path
    }
  }),
  /** Probe a partially-typed root path: whether it already exists (as a dir or a
   *  file), plus child-directory completions for the add-root autocomplete. */
  probePath: (path: string) => call({
    command: "workspace_probe_path",
    schema: PathProbe,
    args: {
      path
    }
  }),
  scan: (root: string) => call({
    command: "workspace_scan",
    schema: z.array(ProjectEntry),
    args: {
      root
    }
  }),
  open: (path: string) => run({
    command: "workspace_open",
    args: {
      path
    }
  }),
  /** Delete a consumed auto-handoff doc (`continue-*.md` in the project dir —
   *  the backend refuses anything else). */
  deleteHandoffDoc: (args: {
    dir: string;
    name: string;
  }) => run({
    command: "handoff_doc_delete",
    args: {
      ...args
    }
  }),
  /** Create a throwaway workspace, mark it owned, and open it — chdirs the
   *  process and records it in Recents. Returns its path. */
  temp: () => call({
    command: "workspace_temp",
    schema: z.string()
  }),
  move: (args: {
    from: string;
    destDir: string;
  }) => call({
    command: "workspace_move",
    schema: z.string(),
    args: {
      ...args
    }
  }),
  rename: (args: {
    from: string;
    newName: string;
  }) => call({
    command: "workspace_rename",
    schema: z.string(),
    args: {
      ...args
    }
  }),
  /** Set a friendly display label (no disk rename). */
  setLabel: (args: {
    path: string;
    name: string;
  }) => call({
    command: "workspace_set_label",
    schema: Settings,
    args: {
      ...args
    }
  }),
  /** Suggest a name for a temp workspace via the agent CLI, else a heuristic. */
  autoname: (args: {
    path: string;
    agent: string;
  }) => call({
    command: "project_autoname",
    schema: z.string().nullable(),
    args: {
      ...args
    }
  }),
  delete: (path: string) => call({
    command: "workspace_delete",
    schema: Settings,
    args: {
      path
    }
  }),
  /** Settings with every vanished folder forgotten (see `dirs`). */
  prune: () => call({
    command: "workspace_prune",
    schema: Settings
  }),
  create: (args: {
    root: string;
    name: string;
  }) => call({
    command: "workspace_create",
    schema: z.string(),
    args: {
      ...args
    }
  }),
  clearRecent: () => call({
    command: "workspace_clear_recent",
    schema: Settings
  }),
  /** Pin or unpin a project in the switcher; returns the refreshed settings. */
  setPinned: (args: {
    path: string;
    pinned: boolean;
  }) => call({
    command: "workspace_set_pinned",
    schema: Settings,
    args: {
      ...args
    }
  }),
  /** Forget a project from the switcher (recents + pins); folder untouched. */
  removeRecent: (path: string) => call({
    command: "workspace_remove_recent",
    schema: Settings,
    args: {
      path
    }
  }),
  /** Persist a drag-reordered pin order (reorders existing pins only). */
  setPinnedOrder: (paths: string[]) => call({
    command: "workspace_set_pinned_order",
    schema: Settings,
    args: {
      paths
    }
  }),
  /** Delete ANY project directory from disk and forget it — the switcher's "Delete
   *  directory". The caller raises a confirmation and releases the folder first. */
  deleteDirectory: (path: string) => call({
    command: "workspace_delete_directory",
    schema: Settings,
    args: {
      path
    }
  }),
  setDefaultAgent: (agent: string) => call({
    command: "set_default_agent",
    schema: Settings,
    args: {
      agent
    }
  }),
  setProjectAgent: (args: {
    path: string;
    agent: string;
  }) =>
    call({
      command: "set_project_agent",
      schema: Settings,
      args: {
        ...args
      }
    }),
  setPrefs: (prefs: Prefs) => call({
    command: "set_prefs",
    schema: Settings,
    args: {
      prefs
    }
  })
};
