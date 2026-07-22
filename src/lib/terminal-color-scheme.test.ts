import { colorSchemeReport, enablesColorSchemeNotifications } from "@/lib/terminal-color-scheme";
import { describe, expect, it } from "vitest";

describe("colorSchemeReport", () => {
  it("reports dark and light through the DEC 2031 status channel", () => {
    expect(colorSchemeReport("dark")).toBe("\x1b[?997;1n");
    expect(colorSchemeReport("light")).toBe("\x1b[?997;2n");
  });

  it("recognises a TUI enabling live color-scheme notifications", () => {
    expect(enablesColorSchemeNotifications("before\x1b[?2031hafter")).toBe(true);
    expect(enablesColorSchemeNotifications("\x1b[?2031l")).toBe(false);
  });
});
