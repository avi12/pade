// What each window-level chord does once `windowShortcutFor` has read it from a
// key event: reload the WebView, open a fresh empty window, focus the left
// rail's project search, or cycle to the previous / next PADE window.

import { Command, sendMessage } from "@/lib/ipc/messaging";
import { WindowShortcut } from "@/lib/shell/shortcuts/window-shortcuts";
import type { windowShortcutFor } from "@/lib/shell/shortcuts/window-shortcuts";
import { showToast } from "@/lib/stores/ui/toast.svelte";
import { WindowMode } from "@/lib/types/window";

type Shortcut = NonNullable<ReturnType<typeof windowShortcutFor>>;

/** The left rail's project search field - what Ctrl+P focuses. */
export const PROJECT_SEARCH_ID = "project-search";

/** Carry out a window-level chord. */
export async function runWindowShortcut(shortcut: Shortcut) {
  if (shortcut === WindowShortcut.RELOAD) {
    location.reload();
    return;
  }

  if (shortcut === WindowShortcut.NEW_WINDOW) {
    await sendMessage(Command.WINDOW_CREATE, { mode: WindowMode.enum.empty });
    showToast("Opened a new window");
    return;
  }

  if (shortcut === WindowShortcut.FIND_PROJECT) {
    const elSearch = document.querySelector<HTMLInputElement>(`input#${PROJECT_SEARCH_ID}`);
    elSearch?.focus();
    elSearch?.select();
    return;
  }

  await sendMessage(Command.WINDOW_FOCUS_RELATIVE, {
    direction: shortcut === WindowShortcut.PREVIOUS_WINDOW ? "previous" : "next"
  });
}
