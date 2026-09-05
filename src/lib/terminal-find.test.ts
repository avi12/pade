import { findResultLabel, isFindShortcut } from "@/lib/terminal-find";
import { describe, expect, it } from "vitest";

function chord(overrides: Partial<{
  key: string;
  ctrlKey: boolean;
  shiftKey: boolean;
  altKey: boolean;
  metaKey: boolean;
}> = {}) {
  return {
    key: "f",
    ctrlKey: false,
    shiftKey: false,
    altKey: false,
    metaKey: false,
    ...overrides
  };
}

describe("isFindShortcut", () => {
  it("matches Ctrl+F whatever case the key arrives in", () => {
    expect(isFindShortcut(chord({ ctrlKey: true }))).toBe(true);
    expect(
      isFindShortcut(
        chord({
          key: "F",
          ctrlKey: true
        })
      )
    ).toBe(true);
  });

  it("leaves a bare F for the agent", () => {
    expect(isFindShortcut(chord())).toBe(false);
  });

  it("leaves the modified chords for the agent", () => {
    // Ctrl+Shift+F and Ctrl+Alt+F are an agent's own bindings — the terminal
    // must keep forwarding them, or opening the find bar would eat them.
    expect(
      isFindShortcut(
        chord({
          ctrlKey: true,
          shiftKey: true
        })
      )
    ).toBe(false);
    expect(
      isFindShortcut(
        chord({
          ctrlKey: true,
          altKey: true
        })
      )
    ).toBe(false);
    expect(
      isFindShortcut(
        chord({
          ctrlKey: true,
          metaKey: true
        })
      )
    ).toBe(false);
  });
});

describe("findResultLabel", () => {
  it("says nothing until something is typed", () => {
    expect(
      findResultLabel({
        term: "",
        resultIndex: -1,
        resultCount: 0
      })
    ).toBe("");
  });

  it("reports a search that found nothing", () => {
    expect(
      findResultLabel({
        term: "cargo",
        resultIndex: -1,
        resultCount: 0
      })
    ).toBe("No matches");
  });

  it("counts from one — the position readout is for a reader, not an index", () => {
    expect(
      findResultLabel({
        term: "cargo",
        resultIndex: 0,
        resultCount: 12
      })
    ).toBe("1 of 12");
    expect(
      findResultLabel({
        term: "cargo",
        resultIndex: 11,
        resultCount: 12
      })
    ).toBe("12 of 12");
  });

  it("shows the total alone once the addon stops tracking which match is active", () => {
    expect(
      findResultLabel({
        term: "e",
        resultIndex: -1,
        resultCount: 1000
      })
    ).toBe("1000 matches");
  });
});
