import { terminalFlushMode, TerminalFlushMode, wheelScrollsTerminalDocument } from "@/lib/terminal/terminal-output";
import { describe, expect, it } from "vitest";

describe("wheelScrollsTerminalDocument", () => {
  it("defers only for a wheel tick moving xterm's own viewport", () => {
    expect(
      wheelScrollsTerminalDocument({
        isAgentOwningMouse: false,
        hasNativeScrollback: true
      })
    )
      .toBe(true);
  });

  it("never defers while a fullscreen agent owns the mouse - the repaint IS the scroll", () => {
    expect(
      wheelScrollsTerminalDocument({
        isAgentOwningMouse: true,
        hasNativeScrollback: false
      })
    )
      .toBe(false);
    // Claude on the alternate screen: grabbed the mouse, and xterm may still hold
    // scrollback from before the switch. The tick is still input, not a scroll.
    expect(
      wheelScrollsTerminalDocument({
        isAgentOwningMouse: true,
        hasNativeScrollback: true
      })
    )
      .toBe(false);
  });

  it("never defers when the tick is forwarded as PageUp/PageDown", () => {
    expect(
      wheelScrollsTerminalDocument({
        isAgentOwningMouse: false,
        hasNativeScrollback: false
      })
    )
      .toBe(false);
  });
});

describe("terminalFlushMode", () => {
  it("tracks frames for the terminal in front", () => {
    expect(
      terminalFlushMode({
        isShown: true,
        isWindowFocused: true,
        isReadingScrollback: false,
        isScrolling: false
      })
    )
      .toBe(TerminalFlushMode.AnimationFrame);
  });

  it("bounds output paints in an unfocused window", () => {
    expect(
      terminalFlushMode({
        isShown: true,
        isWindowFocused: false,
        isReadingScrollback: false,
        isScrolling: false
      })
    )
      .toBe(TerminalFlushMode.Background);
  });

  it("bounds output paints for a hidden tab", () => {
    expect(
      terminalFlushMode({
        isShown: false,
        isWindowFocused: true,
        isReadingScrollback: false,
        isScrolling: false
      })
    )
      .toBe(TerminalFlushMode.Background);
  });

  it("bounds output paints while the user reads scrollback", () => {
    expect(
      terminalFlushMode({
        isShown: true,
        isWindowFocused: true,
        isReadingScrollback: true,
        isScrolling: false
      })
    )
      .toBe(TerminalFlushMode.Background);
  });

  it("defers output paints during an active wheel gesture", () => {
    expect(
      terminalFlushMode({
        isShown: true,
        isWindowFocused: true,
        isReadingScrollback: true,
        isScrolling: true
      })
    )
      .toBe(TerminalFlushMode.Deferred);
  });
});
