// Building a pane's xterm instance: its options, its theme and its links.

import { Command, sendMessage } from "@/lib/ipc/messaging";
import { effective } from "@/lib/settings/prefs.svelte";
import { showToast } from "@/lib/stores/ui/toast.svelte";
import { terminalLinkDestination, TerminalLinkTarget } from "@/lib/terminal/terminal-link-target";
import { registerWrappedLinkProvider } from "@/lib/terminal/terminal-links";
import { rootTokenReader, xtermTheme } from "@/lib/terminal/terminal-theme";
import { Terminal } from "@xterm/xterm";

// xterm's font-size option is in px, so the CSS --ui-scale does not reach it.
const TERMINAL_FONT_SIZE = 13;

// WCAG AA for body text - the floor xterm holds every foreground to against
// the themed background (see the Terminal options).
const MINIMUM_CONTRAST_RATIO = 4.5;

/** The terminal's font size at the current UI zoom. */
export function terminalFontSize() {
  return Math.round(TERMINAL_FONT_SIZE * effective.uiScale);
}

/** xterm's theme as the document's tokens paint it right now. The DOM read lives
 *  here; the token→slot mapping and the parse-safe color conversion (xterm
 *  silently drops formats its parser rejects) are the pure module's job -
 *  lib/terminal/terminal-theme, which also documents the app↔agent theme-sync
 *  decision. */
export function currentXtermTheme() {
  return xtermTheme({ readToken: rootTokenReader() });
}

// Open a terminal hyperlink in the appropriate OS surface. Codex uses OSC-8
// `file:` links for paths it reports; those are local folders/files, not URLs
// the browser bridge accepts. Web links retain the browser path.
async function openTerminalLink(uri: string) {
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

/** A themed xterm, opened into `host`, whose links open in the OS. */
export function createTerminal(host: HTMLElement) {
  const terminal = new Terminal({
    fontFamily: effective.monospaceFamily,
    fontSize: terminalFontSize(),
    cursorBlink: true,
    allowProposedApi: true,
    theme: currentXtermTheme(),
    // Safety net for colors the palette can't remap: an agent that paints
    // truecolor picked for the opposite scheme (a pale blue on the light
    // background) is nudged to WCAG AA against ours. Render-time only - the
    // buffer keeps the agent's true colors.
    minimumContrastRatio: MINIMUM_CONTRAST_RATIO,
    // OSC 8 hyperlinks - the terminal's <a>: an escape-wrapped label with a
    // hidden URL (Claude's "Security guide", "MCP documentation"). xterm
    // detects these itself but only activates them through this handler.
    // Routed through the same bridge as plain-text URLs; the Rust side still
    // refuses anything that isn't http(s), so a file:// or custom-scheme
    // link an agent emits goes nowhere.
    linkHandler: {
      activate(_event, uri) {
        openTerminalLink(uri);
      }
    }
  });
  terminal.open(host);

  // Make URLs in the output clickable - the agent's OAuth sign-in links, docs
  // pointers. xterm's stock web-links addon only rejoins soft-wrapped rows, so
  // a URL a fullscreen agent hard-wraps at the edge would open truncated; this
  // provider stitches those rows too (see terminal-links). The default handler
  // is window.open, which a Tauri WebView won't turn into a browser tab, so
  // route the whole URL through the bridge to the system browser instead.
  registerWrappedLinkProvider({
    terminal,
    openUrl: openTerminalLink
  });
  return terminal;
}
