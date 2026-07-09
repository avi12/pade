// The single boundary between the UI and the Rust core (DRY + SoC).
// No Svelte component calls Tauri directly — everything funnels through here,
// so the IPC contract lives in exactly one place.

import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import type { Agent, ChangeEvent, Commit, ConfigFile, StatusEntry } from "./types";

/** Detected agent backends. */
export const agents = {
  detect: () => invoke<Agent[]>("agents_detect"),
};

interface PtyChunk { id: string; data: string }
interface PtyExit { id: string }

/** Terminal / PTY channel. Every session is addressed by its `id`; callbacks
 *  receive the id so a listener can route to the right terminal. */
export const pty = {
  spawn: (id: string, command: string | null, cols: number, rows: number) =>
    invoke<void>("pty_spawn", { id, command, cols, rows }),
  write: (id: string, data: string) => invoke<void>("pty_write", { id, data }),
  resize: (id: string, cols: number, rows: number) =>
    invoke<void>("pty_resize", { id, cols, rows }),
  kill: (id: string) => invoke<void>("pty_kill", { id }),
  onData: (cb: (id: string, data: string) => void): Promise<UnlistenFn> =>
    listen<PtyChunk>("pty://data", (e) => cb(e.payload.id, e.payload.data)),
  onExit: (cb: (id: string) => void): Promise<UnlistenFn> =>
    listen<PtyExit>("pty://exit", (e) => cb(e.payload.id)),
};

/** Change Feed / filesystem watcher channel. */
export const feed = {
  start: () => invoke<void>("watch_start"),
  onChange: (cb: (ev: ChangeEvent) => void): Promise<UnlistenFn> =>
    listen<ChangeEvent>("feed://change", (e) => cb(e.payload)),
};

/** Version-control review channel. */
export const vcs = {
  status: () => invoke<StatusEntry[]>("vcs_status"),
  log: (limit = 20) => invoke<Commit[]>("vcs_log", { limit }),
  diff: (path: string, staged = false) => invoke<string>("vcs_diff", { path, staged }),
};

/** Agent config channel — reads the CLI's own config files, never shadows them. */
export const config = {
  list: () => invoke<ConfigFile[]>("config_list"),
  read: (rel: string) => invoke<string>("config_read", { rel }),
};
