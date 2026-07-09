// The single boundary between the UI and the Rust core (DRY + SoC).
// No Svelte component calls Tauri directly — everything funnels through here,
// so the IPC contract lives in exactly one place.

import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import type { ChangeEvent } from "./types";

/** Terminal / PTY channel. */
export const pty = {
  spawn: (cols: number, rows: number) => invoke<void>("pty_spawn", { cols, rows }),
  write: (data: string) => invoke<void>("pty_write", { data }),
  resize: (cols: number, rows: number) => invoke<void>("pty_resize", { cols, rows }),
  onData: (cb: (chunk: string) => void): Promise<UnlistenFn> =>
    listen<string>("pty://data", (e) => cb(e.payload)),
  onExit: (cb: () => void): Promise<UnlistenFn> =>
    listen<void>("pty://exit", () => cb()),
};

/** Change Feed / filesystem watcher channel. */
export const feed = {
  start: () => invoke<void>("watch_start"),
  onChange: (cb: (ev: ChangeEvent) => void): Promise<UnlistenFn> =>
    listen<ChangeEvent>("feed://change", (e) => cb(e.payload)),
};
