// Pause continuous CSS animations while the window is being dragged.
//
// WebView2 on Windows repaints the whole webview during an OS window-move; if
// something is also animating every frame (e.g. the "working" status pulse),
// the two contend and the drag stutters. Toggling a class during move lets the
// compositor rest, so dragging stays smooth.

import { getCurrentWindow } from "@tauri-apps/api/window";

const IDLE_MS = 120;

export function pauseAnimationsWhileMoving(): void {
  const root = document.documentElement;
  let timer: ReturnType<typeof setTimeout>;

  void getCurrentWindow().onMoved(() => {
    root.classList.add("window-moving");
    clearTimeout(timer);
    // `onMoved` fires per position step; a short quiet gap means the drag ended.
    timer = setTimeout(() => root.classList.remove("window-moving"), IDLE_MS);
  });
}
