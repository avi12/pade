import { type KeyChord, matchTabShortcut, TabAction } from "@/lib/tabShortcuts";
import { describe, expect, it } from "vitest";

// A chord with every modifier off, overridden per case.
function chord(over: Partial<KeyChord> = {}): KeyChord {
  return {
    key: "",
    ctrlKey: false,
    shiftKey: false,
    altKey: false,
    metaKey: false,
    ...over
  };
}

describe("matchTabShortcut", () => {
  it("opens the launch menu on Ctrl+Alt+T and a new tab on Ctrl+Shift+T", () => {
    expect(
      matchTabShortcut(
        chord({
          ctrlKey: true,
          altKey: true,
          key: "t"
        })
      )
    ).toBe(TabAction.LaunchMenu);
    // Plain Ctrl+T is no longer a tab shortcut — it belongs to the terminal now.
    expect(
      matchTabShortcut(
        chord({
          ctrlKey: true,
          key: "t"
        })
      )
    ).toBeNull();
    expect(
      matchTabShortcut(
        chord({
          ctrlKey: true,
          shiftKey: true,
          key: "T"
        })
      )
    ).toBe(TabAction.New);
  });

  it("closes on Ctrl+W and Ctrl+F4", () => {
    expect(
      matchTabShortcut(
        chord({
          ctrlKey: true,
          key: "w"
        })
      )
    ).toBe(TabAction.Close);
    expect(
      matchTabShortcut(
        chord({
          ctrlKey: true,
          key: "F4"
        })
      )
    ).toBe(TabAction.Close);
  });

  it("cycles with Ctrl+Tab / Ctrl+Shift+Tab", () => {
    expect(
      matchTabShortcut(
        chord({
          ctrlKey: true,
          key: "Tab"
        })
      )
    ).toBe(TabAction.Next);
    expect(
      matchTabShortcut(
        chord({
          ctrlKey: true,
          shiftKey: true,
          key: "Tab"
        })
      )
    ).toBe(
      TabAction.Previous
    );
  });

  it("cycles with Alt+Arrow", () => {
    expect(
      matchTabShortcut(
        chord({
          altKey: true,
          key: "ArrowRight"
        })
      )
    ).toBe(TabAction.Next);
    expect(
      matchTabShortcut(
        chord({
          altKey: true,
          key: "ArrowLeft"
        })
      )
    ).toBe(TabAction.Previous);
  });

  it("ignores plain keys and unrelated chords", () => {
    expect(matchTabShortcut(chord({ key: "t" }))).toBeNull();
    // Ctrl+Shift+N stays with the new-window handler, not here.
    expect(
      matchTabShortcut(
        chord({
          ctrlKey: true,
          shiftKey: true,
          key: "n"
        })
      )
    ).toBeNull();
    // A stray Alt on a Ctrl shortcut disqualifies it — Ctrl+Alt+T is now the
    // launch-menu chord, so close (Ctrl+W) is the example here.
    expect(
      matchTabShortcut(
        chord({
          ctrlKey: true,
          altKey: true,
          key: "w"
        })
      )
    ).toBeNull();
    // Alt+Shift+Arrow is not a cycle (Shift must be absent for Alt+Arrow).
    expect(
      matchTabShortcut(
        chord({
          altKey: true,
          shiftKey: true,
          key: "ArrowRight"
        })
      )
    ).toBeNull();
  });
});
