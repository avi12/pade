// The single boundary between the UI and the Rust core (DRY + SoC).
// Every response is validated with zod, so malformed data fails loudly here
// rather than corrupting the UI. Two helpers own that contract; channels below
// just declare command + schema.

import {
  Agent,
  ChangeEvent,
  Commit,
  ConfigFile,
  DesignTool,
  Ide,
  LaunchContext,
  ProjectEntry,
  PtyChunk,
  PtyExit,
  Settings,
  StatusEntry
} from "./types";
import type { Prefs } from "./types";
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
  suggest: () => call("ide_suggest", z.array(Ide)),
  open: (args: {
    command: string;
    path?: string;
  }) => run("ide_open", { ...args })
};

/** OS integrations — reveal a project in the file manager or a terminal. */
export const os = {
  explorer: (path: string) => run("open_in_explorer", { path }),
  terminal: (path: string) => run("open_in_terminal", { path })
};

/** AI design/UI-generation tools — a roster ranked for the active agent. */
export const design = {
  tools: (agent: string) => call("design_tools", z.array(DesignTool), { agent }),
  open: (url: string) => run("open_url", { url }),
  /** Dock a tool's live UI in the native child webview over the side pane. */
  embed: ({ url, x, y, width, height }: {
    url: string;
    x: number;
    y: number;
    width: number;
    height: number;
  }) =>
    run("design_embed", {
      url,
      bounds: {
        x,
        y,
        width,
        height
      }
    }),
  /** Reposition the docked webview as its host pane moves/resizes. */
  setBounds: ({ x, y, width, height }: {
    x: number;
    y: number;
    width: number;
    height: number;
  }) =>
    run("design_set_bounds", {
      bounds: {
        x,
        y,
        width,
        height
      }
    }),
  /** Park the docked webview off-screen (keeps its session). */
  close: () => run("design_close")
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
  onData: (callback: (id: string, data: string) => void) =>
    on("pty://data", PtyChunk, payload => callback(payload.id, payload.data)),
  onExit: (callback: (id: string) => void) =>
    on("pty://exit", PtyExit, payload => callback(payload.id))
};

/** Change Feed / filesystem watcher channel. */
export const feed = {
  start: () => run("watch_start"),
  onChange: (callback: (event: ChangeEvent) => void) =>
    on("feed://change", ChangeEvent, callback)
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
  worktreeAdd: (args: {
    branch: string;
    create: boolean;
  }) =>
    call("vcs_worktree_add", z.string(), { ...args })
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
  addRoot: (path: string) => call("workspace_add_root", Settings, { path }),
  removeRoot: (path: string) => call("workspace_remove_root", Settings, { path }),
  scan: (root: string) => call("workspace_scan", z.array(ProjectEntry), { root }),
  open: (path: string) => run("workspace_open", { path }),
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
