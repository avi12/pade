// What each window-level chord does once `windowShortcutFor` has read it from a
// key event: reload the WebView, open a fresh empty window, or cycle to the
// previous / next PADE window.

import { Command, sendMessage } from "@/lib/ipc/messaging";
import { WindowShortcut } from "@/lib/shell/window-shortcuts";
import type { windowShortcutFor } from "@/lib/shell/window-shortcuts";
import { showToast } from "@/lib/stores/ui/toast.svelte";
import { WindowMode } from "@/lib/types";

type Shortcut = NonNullable<ReturnType<typeof windowShortcutFor>>;

/** Carry out a window-level chord. */
export async function runWindowShortcut(shortcut: Shortcut) {
  if (shortcut === WindowShortcut.Reload) {
    location.reload();
    return;
  }

  if (shortcut === WindowShortcut.NewWindow) {
    await sendMessage(Command.windowCreate, { mode: WindowMode.enum.empty });
    showToast("Opened a new window");
    return;
  }

  await sendMessage(Command.windowFocusRelative, {
    direction: shortcut === WindowShortcut.PreviousWindow ? "previous" : "next"
  });
}
