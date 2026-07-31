import { terminalFlushMode, TerminalFlushMode } from "@/lib/terminal-output";
import { describe, expect, it } from "vitest";

describe("terminalFlushMode", () => {
  it("tracks frames for the terminal in front", () => {
    expect(
      terminalFlushMode({
        shown: true,
        windowFocused: true,
        readingScrollback: false,
        scrolling: false
      })
    )
      .toBe(TerminalFlushMode.AnimationFrame);
  });

  it("bounds output paints in an unfocused window", () => {
    expect(
      terminalFlushMode({
        shown: true,
        windowFocused: false,
        readingScrollback: false,
        scrolling: false
      })
    )
      .toBe(TerminalFlushMode.Background);
  });

  it("bounds output paints for a hidden tab", () => {
    expect(
      terminalFlushMode({
        shown: false,
        windowFocused: true,
        readingScrollback: false,
        scrolling: false
      })
    )
      .toBe(TerminalFlushMode.Background);
  });

  it("bounds output paints while the user reads scrollback", () => {
    expect(
      terminalFlushMode({
        shown: true,
        windowFocused: true,
        readingScrollback: true,
        scrolling: false
      })
    )
      .toBe(TerminalFlushMode.Background);
  });

  it("defers output paints during an active wheel gesture", () => {
    expect(
      terminalFlushMode({
        shown: true,
        windowFocused: true,
        readingScrollback: true,
        scrolling: true
      })
    )
      .toBe(TerminalFlushMode.Deferred);
  });
});
