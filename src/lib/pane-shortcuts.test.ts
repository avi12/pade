import { matchPaneShortcut, PaneAction } from "@/lib/pane-shortcuts";
import type { KeyChord } from "@/lib/tab-shortcuts";
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

describe("matchPaneShortcut", () => {
  it("cycles with Ctrl+[ (previous) and Ctrl+] (next)", () => {
    expect(
      matchPaneShortcut({
        chord: chord({
          ctrlKey: true,
          key: "["
        }),
        paneCount: 3
      })
    ).toEqual({
      action: PaneAction.Previous
    });
    expect(
      matchPaneShortcut({
        chord: chord({
          ctrlKey: true,
          key: "]"
        }),
        paneCount: 3
      })
    ).toEqual({
      action: PaneAction.Next
    });
  });

  it("selects the nth pane on Ctrl+Alt+1..9 while it exists", () => {
    expect(
      matchPaneShortcut({
        chord: chord({
          ctrlKey: true,
          altKey: true,
          key: "1"
        }),
        paneCount: 4
      })
    ).toEqual({
      action: PaneAction.SelectAt,
      index: 0
    });
    expect(
      matchPaneShortcut({
        chord: chord({
          ctrlKey: true,
          altKey: true,
          key: "4"
        }),
        paneCount: 4
      })
    ).toEqual({
      action: PaneAction.SelectAt,
      index: 3
    });
    expect(
      matchPaneShortcut({
        chord: chord({
          ctrlKey: true,
          altKey: true,
          key: "9"
        }),
        paneCount: 9
      })
    ).toEqual({
      action: PaneAction.SelectAt,
      index: 8
    });
  });

  it("is a no-op for a number past the last pane", () => {
    expect(
      matchPaneShortcut({
        chord: chord({
          ctrlKey: true,
          altKey: true,
          key: "5"
        }),
        paneCount: 3
      })
    ).toBeNull();
    expect(
      matchPaneShortcut({
        chord: chord({
          ctrlKey: true,
          altKey: true,
          key: "2"
        }),
        paneCount: 1
      })
    ).toBeNull();
  });

  it("closes the active pane on Ctrl+Alt+W, either case", () => {
    expect(
      matchPaneShortcut({
        chord: chord({
          ctrlKey: true,
          altKey: true,
          key: "w"
        }),
        paneCount: 2
      })
    ).toEqual({ action: PaneAction.Close });
    expect(
      matchPaneShortcut({
        chord: chord({
          ctrlKey: true,
          altKey: true,
          key: "W"
        }),
        paneCount: 2
      })
    ).toEqual({ action: PaneAction.Close });
  });

  it("is a no-op when there are no panes", () => {
    expect(
      matchPaneShortcut({
        chord: chord({
          ctrlKey: true,
          key: "["
        }),
        paneCount: 0
      })
    ).toBeNull();
    expect(
      matchPaneShortcut({
        chord: chord({
          ctrlKey: true,
          altKey: true,
          key: "1"
        }),
        paneCount: 0
      })
    ).toBeNull();
    expect(
      matchPaneShortcut({
        chord: chord({
          ctrlKey: true,
          altKey: true,
          key: "w"
        }),
        paneCount: 0
      })
    ).toBeNull();
  });

  it("requires the exact modifier set — a stray Alt/Shift/Meta disqualifies the brackets", () => {
    // Ctrl+[ is plain Ctrl; Alt or Shift on it is not the previous-pane chord.
    expect(
      matchPaneShortcut({
        chord: chord({
          ctrlKey: true,
          altKey: true,
          key: "["
        }),
        paneCount: 3
      })
    ).toBeNull();
    expect(
      matchPaneShortcut({
        chord: chord({
          ctrlKey: true,
          shiftKey: true,
          key: "["
        }),
        paneCount: 3
      })
    ).toBeNull();
    expect(
      matchPaneShortcut({
        chord: chord({
          ctrlKey: true,
          metaKey: true,
          key: "]"
        }),
        paneCount: 3
      })
    ).toBeNull();
    // A bare bracket with no Ctrl is nothing.
    expect(
      matchPaneShortcut({
        chord: chord({ key: "[" }),
        paneCount: 3
      })
    ).toBeNull();
  });

  it("requires Ctrl+Alt (no Shift/Meta) for the number and close chords", () => {
    expect(
      matchPaneShortcut({
        chord: chord({
          ctrlKey: true,
          altKey: true,
          shiftKey: true,
          key: "1"
        }),
        paneCount: 4
      })
    ).toBeNull();
    expect(
      matchPaneShortcut({
        chord: chord({
          ctrlKey: true,
          key: "1"
        }),
        paneCount: 4
      })
    ).toBeNull();
    expect(
      matchPaneShortcut({
        chord: chord({
          ctrlKey: true,
          altKey: true,
          metaKey: true,
          key: "w"
        }),
        paneCount: 4
      })
    ).toBeNull();
  });

  it("does not steal the tab shortcuts (plain Ctrl+9 / Ctrl+W)", () => {
    // Plain Ctrl+9 is the tab strip's last-tab chord — panes only claim Ctrl+Alt+9.
    expect(
      matchPaneShortcut({
        chord: chord({
          ctrlKey: true,
          key: "9"
        }),
        paneCount: 4
      })
    ).toBeNull();
    // Plain Ctrl+W closes a tab — panes only claim Ctrl+Alt+W.
    expect(
      matchPaneShortcut({
        chord: chord({
          ctrlKey: true,
          key: "w"
        }),
        paneCount: 4
      })
    ).toBeNull();
  });

  it("ignores 0 and non-mapped keys", () => {
    expect(
      matchPaneShortcut({
        chord: chord({
          ctrlKey: true,
          altKey: true,
          key: "0"
        }),
        paneCount: 4
      })
    ).toBeNull();
    expect(
      matchPaneShortcut({
        chord: chord({
          ctrlKey: true,
          altKey: true,
          key: "["
        }),
        paneCount: 4
      })
    ).toBeNull();
  });
});
