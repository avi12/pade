import { WindowShortcut, windowShortcutFor } from "@/lib/window-shortcuts";
import { describe, expect, it } from "vitest";

function chord(overrides: Partial<Pick<KeyboardEvent, "key" | "code" | "ctrlKey" | "shiftKey" | "altKey">>) {
  return {
    key: "",
    code: "",
    ctrlKey: false,
    shiftKey: false,
    altKey: false,
    ...overrides
  };
}

describe("windowShortcutFor", () => {
  it("reloads on F5 and Ctrl+Shift+R, in development only", () => {
    const functionFive = chord({ key: "F5" });
    const ctrlShiftR = chord({
      key: "R",
      ctrlKey: true,
      shiftKey: true
    });
    expect(
      windowShortcutFor({
        event: functionFive,
        isDevelopment: true
      })
    ).toBe(WindowShortcut.Reload);
    expect(
      windowShortcutFor({
        event: ctrlShiftR,
        isDevelopment: true
      })
    ).toBe(WindowShortcut.Reload);
    expect(
      windowShortcutFor({
        event: functionFive,
        isDevelopment: false
      })
    ).toBeNull();
  });

  it("leaves plain Ctrl+R to the terminal", () => {
    expect(
      windowShortcutFor({
        event: chord({
          key: "r",
          ctrlKey: true
        }),
        isDevelopment: true
      })
    ).toBeNull();
  });

  it("cycles windows on Ctrl+Alt+brackets by key position, never with Shift", () => {
    expect(
      windowShortcutFor({
        event: chord({
          key: "ü",
          code: "BracketLeft",
          ctrlKey: true,
          altKey: true
        }),
        isDevelopment: false
      })
    ).toBe(WindowShortcut.PreviousWindow);
    expect(
      windowShortcutFor({
        event: chord({
          code: "BracketRight",
          ctrlKey: true,
          altKey: true
        }),
        isDevelopment: false
      })
    ).toBe(WindowShortcut.NextWindow);
    expect(
      windowShortcutFor({
        event: chord({
          code: "BracketRight",
          ctrlKey: true,
          altKey: true,
          shiftKey: true
        }),
        isDevelopment: false
      })
    ).toBeNull();
  });

  it("opens a new window on Ctrl+Shift+N", () => {
    expect(
      windowShortcutFor({
        event: chord({
          key: "N",
          ctrlKey: true,
          shiftKey: true
        }),
        isDevelopment: false
      })
    ).toBe(WindowShortcut.NewWindow);
  });
});
