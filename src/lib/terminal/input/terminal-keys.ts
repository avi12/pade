// The pane's keyboard and wheel overrides, layered on xterm's own handling.

import { AgentId } from "@/lib/agent/agent-icon";
import { Command, sendMessage } from "@/lib/ipc/messaging";
import { readClipboardText, writeClipboardText } from "@/lib/ipc/platform";
import { showToast } from "@/lib/stores/ui/toast.svelte";
import { isFindShortcut } from "@/lib/terminal/find/terminal-find";
import { isPromptNewlineShortcut, pathsAsPromptText, PROMPT_NEWLINE } from "@/lib/terminal/input/terminal-input";
import { accumulateWheelNotches } from "@/lib/terminal/input/terminal-scroll";
import { ERASE_WORD, PAGE_DOWN, PAGE_UP } from "@/lib/terminal/input/terminal-sequences";
import { wheelScrollsTerminalDocument } from "@/lib/terminal/output/terminal-output";
import { Key } from "@/lib/ui/keys";
import type { Terminal } from "@xterm/xterm";

// The terminal's own clipboard chords (Ctrl+C with a selection, Ctrl+V),
// matched case-insensitively.
const COPY_LETTER = "c";
const PASTE_LETTER = "v";
const KEYDOWN_EVENT = "keydown";

// Mouse-tracking mode xterm reports when no program has grabbed the mouse.
const NO_MOUSE_TRACKING = "none";

// Agents whose Shift+Enter newline must arrive as a raw Ctrl+J (LF, 0x0a)
// rather than a bracketed paste of it:
//  • Claude Code - Ctrl+J is its documented always-works insert-newline; a
//    bracketed paste of a lone LF does NOT register in its composer.
//  • OpenCode - reacts to ANY paste event by consulting the OS clipboard (a
//    lingering image would attach alongside the newline); a raw LF is its own
//    ctrl+j insert-newline binding, so the newline lands without a paste.
const NEWLINE_VIA_RAW_LF_AGENTS = new Set<string>([AgentId.Claude, AgentId.Opencode]);

type PtyWrite = (data: string) => Promise<void>;

function isPlainCtrl(event: KeyboardEvent) {
  return event.ctrlKey && !event.shiftKey && !event.altKey && !event.metaKey;
}

/** Keyboard overrides; returning false stops xterm from also sending the key's
 *  control code.
 *   • Shift+Enter → a protected prompt newline instead of submitting.
 *   • Ctrl+C → copy the selection; with nothing selected it falls through so
 *     xterm still sends ^C (SIGINT) to interrupt the agent.
 *   • Ctrl+V → paste the clipboard (xterm would otherwise send a raw ^V, and
 *     only the WebView's right-click menu pasted).
 *   • Ctrl+F → the pane's find bar (lib/terminal/find/TerminalFind), not a raw ^F.
 *   • Ctrl+Backspace → ^W (erase word). xterm's legacy encoding for the
 *     chord is ^H, indistinguishable from a bare backspace to a TUI reading
 *     plain bytes (Windows Terminal gets away with it via win32-input-mode,
 *     which xterm doesn't speak); ^W is the erase-word every line editor
 *     and agent already binds. */
export function attachKeyOverrides({ terminal, agentId, write, openFind }: {
  terminal: Terminal;
  agentId: () => string;
  write: PtyWrite;
  /** Open the pane's find bar. */
  openFind: () => unknown;
}) {
  async function pasteClipboard() {
    try {
      // An image on the clipboard beats text: the backend saves it as a PNG
      // and the pasted *path* is what agent composers attach.
      const imagePath = await sendMessage(Command.clipboardImageSave);
      if (imagePath) {
        terminal.paste(pathsAsPromptText([imagePath]));
        return;
      }

      const text = await readClipboardText();
      if (text) {
        // paste (not write) so xterm wraps it in bracketed-paste markers when the
        // agent has that mode on - it then treats it as pasted text, not typing.
        terminal.paste(text);
      }
    } catch {
      showToast("Couldn't read the clipboard");
    }
  }

  // Send the current selection to the clipboard. Copy is best-effort - a
  // clipboard the platform withholds must not throw into a key handler.
  async function copySelectionToClipboard() {
    try {
      await writeClipboardText(terminal.getSelection());
    } catch {
      showToast("Couldn't copy the terminal selection");
    }
  }

  function insertPromptNewline() {
    if (NEWLINE_VIA_RAW_LF_AGENTS.has(agentId())) {
      // A raw Ctrl+J (LF) is the reliable insert-newline for these agents -
      // Claude Code's always-works binding, and OpenCode's, without tripping
      // its clipboard-on-paste. A bracketed paste of a lone LF is silently
      // dropped by Claude's composer, which is why Shift+Enter did nothing.
      write(PROMPT_NEWLINE);
      return;
    }

    // `paste` is deliberate: when any agent's TUI enables bracketed paste,
    // xterm marks this as pasted text and the newline stays in its composer.
    // Unlike CSI-u, this does not depend on an individual agent decoding a
    // particular modified-key protocol.
    terminal.paste(PROMPT_NEWLINE);
  }

  // The Ctrl-only chords (no Shift, Alt or Meta), by lowercased key.
  const plainCtrlActions: Record<string, () => unknown> = {
    [PASTE_LETTER]: pasteClipboard,
    [Key.Backspace.toLowerCase()]: () => write(ERASE_WORD)
  };

  // Ctrl+C acts only with a selection, so without one it falls through to
  // xterm's ^C.
  function plainCtrlAction(key: string) {
    const lowercaseKey = key.toLowerCase();
    if (lowercaseKey === COPY_LETTER) {
      return terminal.hasSelection() ? copySelectionToClipboard : null;
    }

    return plainCtrlActions[lowercaseKey] ?? null;
  }

  function keyOverride(event: KeyboardEvent) {
    if (isPromptNewlineShortcut(event)) {
      return insertPromptNewline;
    }

    return plainCtrlOverride(event) ?? findOverride(event);
  }

  function plainCtrlOverride(event: KeyboardEvent) {
    return isPlainCtrl(event) ? plainCtrlAction(event.key) : null;
  }

  // Ctrl+F searches this pane's output instead of reaching the agent as a raw
  // ^F (which a shell reads as "forward one character").
  function findOverride(event: KeyboardEvent) {
    return isFindShortcut(event) ? openFind : null;
  }

  terminal.attachCustomKeyEventHandler(event => {
    const override = event.type === KEYDOWN_EVENT ? keyOverride(event) : null;
    if (!override) {
      return true;
    }

    // preventDefault also stops the browser inserting a newline into xterm's
    // hidden textarea on Shift+Enter, which xterm would forward as a submit.
    event.preventDefault();
    override();
    return false;
  });
}

/** Wheel-scroll a fullscreen agent's own transcript when xterm has nothing to
 *  scroll itself. Claude Code's renderer repaints its UI in place (cursor
 *  addressing) rather than appending lines, so the earlier conversation lives
 *  inside the agent, not in xterm's buffer - there is nothing in xterm's
 *  scrollback for a wheel tick to reveal. The agent scrolls its transcript a
 *  half-page per PageUp/PageDown and says so on screen ("use PgUp/PgDn to
 *  scroll"); the arrow keys the terminal would otherwise emit for a wheel tick
 *  instead walk the prompt's input history - the well-known hijack
 *  (claude-code#65833). So a wheel tick with nothing to reveal is forwarded as
 *  PageUp/PageDown.
 *
 *  Defer to xterm - return true, behave as if unhooked - in the two cases it can
 *  handle: a program that grabbed the mouse (Neovim, a pager, an agent doing its
 *  own wheel handling) wants the wheel as a mouse report, and a plain shell with
 *  real scrollback (baseY > 0) wants its own document scrolled. */
export function attachWheelForwarding({ terminal, write, onDocumentScroll }: {
  terminal: Terminal;
  write: PtyWrite;
  /** A tick is about to move xterm's own document. */
  onDocumentScroll: () => void;
}) {
  let wheelCarry = 0;

  // Send the whole notches this tick adds up to as PageUp/PageDown.
  function forwardWheel(e: WheelEvent) {
    const { notches, carry } = accumulateWheelNotches({
      deltaY: e.deltaY,
      deltaMode: e.deltaMode,
      carry: wheelCarry
    });
    wheelCarry = carry;

    if (notches !== 0) {
      const scrollKey = notches < 0 ? PAGE_UP : PAGE_DOWN;
      write(scrollKey.repeat(Math.abs(notches)));
    }
  }

  terminal.attachCustomWheelEventHandler(e => {
    const isAgentOwningMouse = terminal.modes.mouseTrackingMode !== NO_MOUSE_TRACKING;
    const hasNativeScrollback = terminal.buffer.active.baseY > 0;
    // Only a tick that moves xterm's own document may hold output back; a tick
    // the agent receives as input must not, or the repaint that IS the scroll
    // is what gets deferred (see wheelScrollsTerminalDocument).
    if (wheelScrollsTerminalDocument({
      isAgentOwningMouse,
      hasNativeScrollback
    })) {
      onDocumentScroll();
    }

    if (isAgentOwningMouse || hasNativeScrollback) {
      return true;
    }

    forwardWheel(e);
    e.preventDefault();
    return false;
  });
}
