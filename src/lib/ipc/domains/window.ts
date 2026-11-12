import { NO_REPLY } from "@/lib/ipc/domains/no-reply";
import type { PathData } from "@/lib/ipc/domains/path-data";
import { Scheme } from "@/lib/types/theme";
import { DragDropPayload, DragOverPayload, LaunchContext, WindowInfo } from "@/lib/types/window";
import type { WindowDirection, WindowMode } from "@/lib/types/window";
import { z } from "zod";

export const WINDOW_COMMANDS = {
  restart: "restart",
  recoveryArm: "recovery_arm",
  windowCreate: "window_create",
  windowRegisterProject: "window_register_project",
  windowFocusProject: "window_focus_project",
  windowFocusRelative: "window_focus_relative",
  windowList: "window_list",
  windowReorder: "window_reorder",
  windowFocusLabel: "window_focus_label",
  windowTheme: "window_theme",
  windowHandOff: "window_hand_off",
  windowTakeHandoffs: "window_take_handoffs",
  launchContext: "launch_context"
} as const;

export const WINDOW_EVENTS = {
  windowsChanged: "windows://changed",
  windowHandoff: "window://handoff",
  dragOver: "tauri://drag-over",
  dragDrop: "tauri://drag-drop",
  dragLeave: "tauri://drag-leave"
} as const;

export interface WindowCommandProtocol {
  /** Restart PADE so launch-only settings can take effect. */
  [WINDOW_COMMANDS.restart]: () => void;
  /** Arm WebView2 crash auto-recovery with this window's live URL - the one
   *  value a dead browser process can no longer report. */
  [WINDOW_COMMANDS.recoveryArm]: (data: { url: string }) => void;
  /** Spawn a fresh app window targeting a project, an empty picker, or a
   *  throwaway workspace; it routes off its query string. */
  [WINDOW_COMMANDS.windowCreate]: (data: {
    mode: WindowMode;
    path?: string;
    /** A `handoff` window takes these live sessions of the calling window... */
    sessions?: string[];
    /** ...and this description of them (lib/shell/handoff/window-handoff). */
    payload?: string;
  }) => void;
  /** Record the project this window now has open (focus-instead-of-reopen). */
  [WINDOW_COMMANDS.windowRegisterProject]: (data: PathData) => void;
  /** Focus another window already showing this project; true if one was found. */
  [WINDOW_COMMANDS.windowFocusProject]: (data: PathData) => boolean;
  /** Focus the previous/next window in stable creation order; false when this
   *  is the only one. */
  [WINDOW_COMMANDS.windowFocusRelative]: (data: { direction: WindowDirection }) => boolean;
  /** Every open window that has a project, in the user's explicit order. */
  [WINDOW_COMMANDS.windowList]: () => WindowInfo[];
  /** Persist the drag-reordered "Open windows" order - the one source both
   *  that list and the Ctrl+Alt+[ / ] cycle read. */
  [WINDOW_COMMANDS.windowReorder]: (data: { labels: string[] }) => void;
  /** Focus a specific open window by its label; true if it existed. */
  [WINDOW_COMMANDS.windowFocusLabel]: (data: { label: string }) => boolean;
  /** This window's OS theme from the native side - the authoritative answer at
   *  startup, where WebView2's media query can still say light (window.rs). */
  [WINDOW_COMMANDS.windowTheme]: () => Scheme;
  /** Move this window's live sessions to the open window `to` (same PTYs, still
   *  running), park the payload describing them there and focus it; false when
   *  `to` is not another open window. */
  [WINDOW_COMMANDS.windowHandOff]: (data: {
    to: string;
    sessions: string[];
    payload: string;
  }) => boolean;
  /** The hand-off payloads parked for this window, taken as they are read. */
  [WINDOW_COMMANDS.windowTakeHandoffs]: () => string[];
  [WINDOW_COMMANDS.launchContext]: () => LaunchContext;
}

export const WINDOW_RESPONSE_SCHEMAS = {
  [WINDOW_COMMANDS.restart]: NO_REPLY,
  [WINDOW_COMMANDS.recoveryArm]: NO_REPLY,
  [WINDOW_COMMANDS.windowCreate]: NO_REPLY,
  [WINDOW_COMMANDS.windowRegisterProject]: NO_REPLY,
  [WINDOW_COMMANDS.windowFocusProject]: z.boolean(),
  [WINDOW_COMMANDS.windowFocusRelative]: z.boolean(),
  [WINDOW_COMMANDS.windowList]: z.array(WindowInfo),
  [WINDOW_COMMANDS.windowReorder]: NO_REPLY,
  [WINDOW_COMMANDS.windowFocusLabel]: z.boolean(),
  [WINDOW_COMMANDS.windowTheme]: Scheme,
  [WINDOW_COMMANDS.windowHandOff]: z.boolean(),
  [WINDOW_COMMANDS.windowTakeHandoffs]: z.array(z.string()),
  [WINDOW_COMMANDS.launchContext]: LaunchContext
} satisfies { [Key in keyof WindowCommandProtocol]: z.ZodType<ReturnType<WindowCommandProtocol[Key]>> };

export interface WindowEventProtocol {
  /** The open-windows set/order changed in SOME window (broadcast). */
  [WINDOW_EVENTS.windowsChanged]: (data: null) => void;
  /** Another window handed sessions to this one - take them. */
  [WINDOW_EVENTS.windowHandoff]: (data: null) => void;
  /** Native OS drag-and-drop onto the window: Tauri intercepts the webview's
   *  file DnD (the web API never exposes absolute paths) and emits these. */
  [WINDOW_EVENTS.dragOver]: (data: DragOverPayload) => void;
  [WINDOW_EVENTS.dragDrop]: (data: DragDropPayload) => void;
  [WINDOW_EVENTS.dragLeave]: (data: unknown) => void;
}

export const WINDOW_PAYLOAD_SCHEMAS = {
  [WINDOW_EVENTS.windowsChanged]: z.null(),
  [WINDOW_EVENTS.windowHandoff]: z.null(),
  [WINDOW_EVENTS.dragOver]: DragOverPayload,
  [WINDOW_EVENTS.dragDrop]: DragDropPayload,
  [WINDOW_EVENTS.dragLeave]: z.unknown()
} satisfies { [Key in keyof WindowEventProtocol]: z.ZodType<Parameters<WindowEventProtocol[Key]>[0]> };
