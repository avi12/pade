// Send-from-IDE bridge: highlight + copy a snippet in any external editor,
// then press the global shortcut to inject the clipboard into the active
// agent's input — works regardless of which IDE the project is open in. The
// app shell registers on mount and unregisters on destroy; the active session
// comes through `SendShortcutHost` so the lookup stays live.

import { pty } from "@/lib/bridge";
import { showToast } from "@/lib/stores/toast.svelte";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { readText } from "@tauri-apps/plugin-clipboard-manager";
import { register, unregister } from "@tauri-apps/plugin-global-shortcut";

const SEND_SHORTCUT = "CommandOrControl+Alt+S";

/** What the app shell provides — read at each press, so the target follows the
 *  currently active session. */
export interface SendShortcutHost {
  activeId: () => string | null;
  /** The active session's agent label, for the confirmation toast. */
  activeLabel: () => string;
}

/** Register the global send shortcut (idempotent — cleans a stale registration
 *  first, so an HMR re-register never doubles up). */
export async function registerSendShortcut(host: SendShortcutHost): Promise<void> {
  await unregisterSendShortcut();
  await register(SEND_SHORTCUT, async event => {
    if (event.state !== "Pressed") {
      return;
    }

    const text = (await readText()).trim();
    const activeId = host.activeId();
    if (!text || !activeId) {
      return;
    }

    await pty.write({
      id: activeId,
      data: text
    });
    await getCurrentWindow().setFocus();
    showToast(`Sent selection to ${host.activeLabel()}`);
  });
}

/** Release the global shortcut (safe when it was never registered). */
export async function unregisterSendShortcut(): Promise<void> {
  await unregister(SEND_SHORTCUT).catch(() => {});
}
