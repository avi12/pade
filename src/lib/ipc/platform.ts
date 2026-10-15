// The Tauri APIs that are not app commands: this window's own identity and
// lifecycle, and the clipboard plugin. Kept beside the messaging layer so the
// UI still reaches Tauri through `@/lib/ipc` alone.

import { getCurrentWindow } from "@tauri-apps/api/window";

export { readText as readClipboardText, writeText as writeClipboardText } from "@tauri-apps/plugin-clipboard-manager";

/** This window's Tauri label - the id the backend scopes its per-window state
 *  to (PTY ownership, targeted emits), and stable across a crash-recovery
 *  rebuild, which recreates the webview under the same label. Anything that
 *  outlives the webview (the session snapshot) must be scoped to it. */
export function windowLabel() {
  return getCurrentWindow().label;
}

/** Set this window's OS title (title bar + taskbar) - the one place the UI
 *  drives Tauri's window title, so no surface recomputes it independently. */
export function setWindowTitle(title: string) {
  return getCurrentWindow().setTitle(title);
}

/** Intercept this window's close (the title-bar X): the handler runs to
 *  completion first - the graceful-leave hook - and only then is the window
 *  destroyed (Tauri's own onCloseRequested contract when nothing prevents).
 *  Resolves to the unlisten function. */
export function onWindowCloseRequested(handler: () => Promise<void>) {
  return getCurrentWindow().onCloseRequested(handler);
}
