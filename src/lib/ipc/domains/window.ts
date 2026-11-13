import { z } from "zod";

import { NO_REPLY } from "@/lib/ipc/domains/no-reply";
import type { PathData } from "@/lib/ipc/domains/path-data";
import { Scheme } from "@/lib/types/theme";
import { DragDropPayload, DragOverPayload, LaunchContext, WindowInfo } from "@/lib/types/window";
import type { WindowDirection, WindowMode } from "@/lib/types/window";

export const WINDOW_COMMANDS = {
  RESTART: "restart",
  RECOVERY_ARM: "recovery_arm",
  WINDOW_CREATE: "window_create",
  WINDOW_REGISTER_PROJECT: "window_register_project",
  WINDOW_FOCUS_PROJECT: "window_focus_project",
  WINDOW_FOCUS_RELATIVE: "window_focus_relative",
  WINDOW_LIST: "window_list",
  WINDOW_REORDER: "window_reorder",
  WINDOW_FOCUS_LABEL: "window_focus_label",
  WINDOW_THEME: "window_theme",
  WINDOW_HAND_OFF: "window_hand_off",
  WINDOW_TAKE_HANDOFFS: "window_take_handoffs",
  LAUNCH_CONTEXT: "launch_context"
} as const;

export const WINDOW_EVENTS = {
  WINDOWS_CHANGED: "windows://changed",
  WINDOW_HANDOFF: "window://handoff",
  DRAG_OVER: "tauri://drag-over",
  DRAG_DROP: "tauri://drag-drop",
  DRAG_LEAVE: "tauri://drag-leave"
} as const;

export interface WindowCommandProtocol {
  /** Restart PADE so launch-only settings can take effect. */
  [WINDOW_COMMANDS.RESTART]: () => void;
  /** Arm WebView2 crash auto-recovery with this window's live URL - the one
   *  value a dead browser process can no longer report. */
  [WINDOW_COMMANDS.RECOVERY_ARM]: (data: { url: string }) => void;
  /** Spawn a fresh app window targeting a project, an empty picker, or a
   *  throwaway workspace; it routes off its query string. */
  [WINDOW_COMMANDS.WINDOW_CREATE]: (data: {
    mode: WindowMode;
    path?: string;
    /** A `handoff` window takes these live sessions of the calling window... */
    sessions?: string[];
    /** ...and this description of them (lib/shell/handoff/window-handoff). */
    payload?: string;
  }) => void;
  /** Record the project this window now has open (focus-instead-of-reopen). */
  [WINDOW_COMMANDS.WINDOW_REGISTER_PROJECT]: (data: PathData) => void;
  /** Focus another window already showing this project; true if one was found. */
  [WINDOW_COMMANDS.WINDOW_FOCUS_PROJECT]: (data: PathData) => boolean;
  /** Focus the previous/next window in stable creation order; false when this
   *  is the only one. */
  [WINDOW_COMMANDS.WINDOW_FOCUS_RELATIVE]: (data: { direction: WindowDirection }) => boolean;
  /** Every open window that has a project, in the user's explicit order. */
  [WINDOW_COMMANDS.WINDOW_LIST]: () => WindowInfo[];
  /** Persist the drag-reordered "Open windows" order - the one source both
   *  that list and the Ctrl+Alt+[ / ] cycle read. */
  [WINDOW_COMMANDS.WINDOW_REORDER]: (data: { labels: string[] }) => void;
  /** Focus a specific open window by its label; true if it existed. */
  [WINDOW_COMMANDS.WINDOW_FOCUS_LABEL]: (data: { label: string }) => boolean;
  /** This window's OS theme from the native side - the authoritative answer at
   *  startup, where WebView2's media query can still say light (window.rs). */
  [WINDOW_COMMANDS.WINDOW_THEME]: () => Scheme;
  /** Move this window's live sessions to the open window `to` (same PTYs, still
   *  running), park the payload describing them there and focus it; false when
   *  `to` is not another open window. */
  [WINDOW_COMMANDS.WINDOW_HAND_OFF]: (data: { to: string; sessions: string[]; payload: string }) => boolean;
  /** The hand-off payloads parked for this window, taken as they are read. */
  [WINDOW_COMMANDS.WINDOW_TAKE_HANDOFFS]: () => string[];
  [WINDOW_COMMANDS.LAUNCH_CONTEXT]: () => LaunchContext;
}

export const WINDOW_RESPONSE_SCHEMAS = {
  [WINDOW_COMMANDS.RESTART]: NO_REPLY,
  [WINDOW_COMMANDS.RECOVERY_ARM]: NO_REPLY,
  [WINDOW_COMMANDS.WINDOW_CREATE]: NO_REPLY,
  [WINDOW_COMMANDS.WINDOW_REGISTER_PROJECT]: NO_REPLY,
  [WINDOW_COMMANDS.WINDOW_FOCUS_PROJECT]: z.boolean(),
  [WINDOW_COMMANDS.WINDOW_FOCUS_RELATIVE]: z.boolean(),
  [WINDOW_COMMANDS.WINDOW_LIST]: z.array(WindowInfo),
  [WINDOW_COMMANDS.WINDOW_REORDER]: NO_REPLY,
  [WINDOW_COMMANDS.WINDOW_FOCUS_LABEL]: z.boolean(),
  [WINDOW_COMMANDS.WINDOW_THEME]: Scheme,
  [WINDOW_COMMANDS.WINDOW_HAND_OFF]: z.boolean(),
  [WINDOW_COMMANDS.WINDOW_TAKE_HANDOFFS]: z.array(z.string()),
  [WINDOW_COMMANDS.LAUNCH_CONTEXT]: LaunchContext
} satisfies { [Key in keyof WindowCommandProtocol]: z.ZodType<ReturnType<WindowCommandProtocol[Key]>> };

export interface WindowEventProtocol {
  /** The open-windows set/order changed in SOME window (broadcast). */
  [WINDOW_EVENTS.WINDOWS_CHANGED]: (data: null) => void;
  /** Another window handed sessions to this one - take them. */
  [WINDOW_EVENTS.WINDOW_HANDOFF]: (data: null) => void;
  /** Native OS drag-and-drop onto the window: Tauri intercepts the webview's
   *  file DnD (the web API never exposes absolute paths) and emits these. */
  [WINDOW_EVENTS.DRAG_OVER]: (data: DragOverPayload) => void;
  [WINDOW_EVENTS.DRAG_DROP]: (data: DragDropPayload) => void;
  [WINDOW_EVENTS.DRAG_LEAVE]: (data: unknown) => void;
}

export const WINDOW_PAYLOAD_SCHEMAS = {
  [WINDOW_EVENTS.WINDOWS_CHANGED]: z.null(),
  [WINDOW_EVENTS.WINDOW_HANDOFF]: z.null(),
  [WINDOW_EVENTS.DRAG_OVER]: DragOverPayload,
  [WINDOW_EVENTS.DRAG_DROP]: DragDropPayload,
  [WINDOW_EVENTS.DRAG_LEAVE]: z.unknown()
} satisfies { [Key in keyof WindowEventProtocol]: z.ZodType<Parameters<WindowEventProtocol[Key]>[0]> };
