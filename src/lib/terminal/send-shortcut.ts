// Send-from-IDE bridge: highlight + copy a snippet in any external editor,
// then press the global shortcut to inject the clipboard into the active
// agent's input - works regardless of which IDE the project is open in. The
// app shell registers on mount and unregisters on destroy; the active session
// comes through `SendShortcutHost` so the lookup stays live.

import { Command, sendMessage } from "@/lib/ipc/messaging";
import { focusWindow, readClipboardText } from "@/lib/ipc/platform";
import { showToast } from "@/lib/stores/ui/toast.svelte";
import { pastedText } from "@/lib/terminal/terminal-input";
import { register, unregister } from "@tauri-apps/plugin-global-shortcut";

const SEND_SHORTCUT = "CommandOrControl+Alt+S";
const PRESSED_STATE = "Pressed";

/** What the app shell provides - read at each press, so the target follows the
 *  currently active session. */
export interface SendShortcutHost {
  activeId: () => string | null;
  /** The active session's agent label, for the confirmation toast. */
  activeLabel: () => string;
}

/** Register the global send shortcut (idempotent - cleans a stale registration
 *  first, so an HMR re-register never doubles up). */
export async function registerSendShortcut(host: SendShortcutHost) {
  await unregisterSendShortcut();
  await register(SEND_SHORTCUT, async event => {
    if (event.state !== PRESSED_STATE) {
      return;
    }

    const text = (await readClipboardText()).trim();
    const activeId = host.activeId();
    if (!text || !activeId) {
      return;
    }

    await sendMessage(Command.ptyWrite, {
      id: activeId,
      data: pastedText(text)
    });
    await focusWindow();
    showToast(`Sent selection to ${host.activeLabel()}`);
  });
}

/** Release the global shortcut. Unregistering one that was never registered
 *  rejects, which is the expected first-mount case rather than a failure. */
export async function unregisterSendShortcut() {
  try {
    await unregister(SEND_SHORTCUT);
  } catch (error) {
    console.debug("pade: the send shortcut was not registered", error);
  }
}
