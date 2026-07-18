import { type KeyChord, matchTabSelection, matchTabShortcut, TabAction } from "@/lib/tab-shortcuts";
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

describe("matchTabSelection", () => {
  // A plain Ctrl chord for the given digit key.
  function ctrlDigit(key: string): KeyChord {
    return chord({
      ctrlKey: true,
      key
    });
  }

  it("maps Ctrl+1..8 to that 0-based tab when it exists", () => {
    expect(
      matchTabSelection({
        chord: ctrlDigit("1"),
        count: 5
      })
    ).toBe(0);
    expect(
      matchTabSelection({
        chord: ctrlDigit("2"),
        count: 5
      })
    ).toBe(1);
    expect(
      matchTabSelection({
        chord: ctrlDigit("8"),
        count: 8
      })
    ).toBe(7);
  });

  it("maps Ctrl+9 to the LAST tab, not the ninth", () => {
    expect(
      matchTabSelection({
        chord: ctrlDigit("9"),
        count: 3
      })
    ).toBe(2);
    expect(
      matchTabSelection({
        chord: ctrlDigit("9"),
        count: 5
      })
    ).toBe(4);
    expect(
      matchTabSelection({
        chord: ctrlDigit("9"),
        count: 12
      })
    ).toBe(11);
  });

  it("is a no-op for a number past the last tab", () => {
    expect(
      matchTabSelection({
        chord: ctrlDigit("5"),
        count: 3
      })
    ).toBeNull();
    expect(
      matchTabSelection({
        chord: ctrlDigit("8"),
        count: 2
      })
    ).toBeNull();
  });

  it("is a no-op when there are no tabs", () => {
    expect(
      matchTabSelection({
        chord: ctrlDigit("1"),
        count: 0
      })
    ).toBeNull();
    expect(
      matchTabSelection({
        chord: ctrlDigit("9"),
        count: 0
      })
    ).toBeNull();
  });

  it("requires plain Ctrl — Shift / Alt / Meta or no Ctrl disqualifies", () => {
    expect(
      matchTabSelection({
        chord: chord({ key: "1" }),
        count: 5
      })
    ).toBeNull();
    expect(
      matchTabSelection({
        chord: chord({
          ctrlKey: true,
          shiftKey: true,
          key: "1"
        }),
        count: 5
      })
    ).toBeNull();
    expect(
      matchTabSelection({
        chord: chord({
          ctrlKey: true,
          altKey: true,
          key: "1"
        }),
        count: 5
      })
    ).toBeNull();
    expect(
      matchTabSelection({
        chord: chord({
          metaKey: true,
          key: "1"
        }),
        count: 5
      })
    ).toBeNull();
  });

  it("ignores 0 and non-digit keys", () => {
    expect(
      matchTabSelection({
        chord: ctrlDigit("0"),
        count: 5
      })
    ).toBeNull();
    expect(
      matchTabSelection({
        chord: ctrlDigit("t"),
        count: 5
      })
    ).toBeNull();
  });
});
