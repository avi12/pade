// The single boundary between the UI and the Rust core (DRY + SoC).
// Every response is validated with zod, so malformed data fails loudly here
// rather than corrupting the UI. Two helpers own that contract; channels below
// just declare command + schema.

import {
  Agent,
  ChangeEvent,
  Commit,
  ConfigFile,
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
  open: (command: string) => run("ide_open", { command })
};

/** Terminal / PTY channel. Sessions are addressed by `id`; callbacks receive it
 *  so a listener can route to the right terminal. */
export const pty = {
  spawn: (id: string, command: string | null, cols: number, rows: number) =>
    run("pty_spawn", {
      id,
      command,
      cols,
      rows
    }),
  write: (id: string, data: string) => run("pty_write", {
    id,
    data
  }),
  resize: (id: string, cols: number, rows: number) => run("pty_resize", {
    id,
    cols,
    rows
  }),
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
  diff: (path: string, staged = false) => call("vcs_diff", z.string(), {
    path,
    staged
  })
};

/** Agent config channel — reads the CLI's own config files, never shadows them. */
export const config = {
  list: () => call("config_list", z.array(ConfigFile)),
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
  create: (root: string, name: string) => call("workspace_create", z.string(), {
    root,
    name
  }),
  clone: (root: string, url: string) => call("workspace_clone", z.string(), {
    root,
    url
  }),
  setDefaultAgent: (agent: string) => call("set_default_agent", Settings, { agent }),
  setProjectAgent: (path: string, agent: string) =>
    call("set_project_agent", Settings, {
      path,
      agent
    }),
  setPrefs: (prefs: Prefs) => call("set_prefs", Settings, { prefs })
};
