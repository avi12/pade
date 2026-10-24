// Opening a link clicked in terminal or runner output, kept apart from xterm so
// the plain-HTML runner output can use it without pulling xterm in.

import { Command, sendMessage } from "@/lib/ipc/messaging";
import { showToast } from "@/lib/stores/ui/toast.svelte";
import { terminalLinkDestination, TerminalLinkTarget } from "@/lib/terminal/links/terminal-link-target";

// Open a terminal hyperlink in the appropriate OS surface. Codex uses OSC-8
// `file:` links for paths it reports; those are local folders/files, not URLs
// the browser bridge accepts. Web links retain the browser path.
export async function openTerminalLink(uri: string) {
  const destination = terminalLinkDestination(uri);
  if (!destination) {
    showToast("Couldn't open that link");
    return;
  }

  try {
    if (destination.kind === TerminalLinkTarget.Explorer) {
      await sendMessage(Command.openInExplorer, { path: destination.value });
    } else {
      await sendMessage(Command.openUrl, { url: destination.value });
    }
  } catch {
    showToast("Couldn't open that link");
  }
}
