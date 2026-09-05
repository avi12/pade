import {
  FindOption,
  findResultLabel,
  isFindShortcut,
  isSearchablePattern,
  matchFindOptionChord
} from "@/lib/terminal-find";
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

  it("says a half-written pattern is not a search that found nothing", () => {
    expect(
      findResultLabel({
        term: "(unclosed",
        resultIndex: -1,
        resultCount: 0,
        searchable: false
      })
    ).toBe("Bad pattern");
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

describe("matchFindOptionChord", () => {
  it("flips a toggle on Alt + its letter", () => {
    expect(
      matchFindOptionChord(
        chord({
          key: "c",
          altKey: true
        })
      )
    ).toBe(FindOption.CaseSensitive);
    expect(
      matchFindOptionChord(
        chord({
          key: "W",
          altKey: true
        })
      )
    ).toBe(FindOption.WholeWord);
    expect(
      matchFindOptionChord(
        chord({
          key: "r",
          altKey: true
        })
      )
    ).toBe(FindOption.Regex);
  });

  it("leaves a plain letter to be typed into the box", () => {
    expect(matchFindOptionChord(chord({ key: "c" }))).toBeNull();
  });

  it("ignores Ctrl+Alt — that is AltGr, which types characters people search for", () => {
    expect(
      matchFindOptionChord(
        chord({
          key: "c",
          altKey: true,
          ctrlKey: true
        })
      )
    ).toBeNull();
  });

  it("is null for a letter no toggle claims", () => {
    expect(
      matchFindOptionChord(
        chord({
          key: "q",
          altKey: true
        })
      )
    ).toBeNull();
  });
});

describe("isSearchablePattern", () => {
  it("takes any literal, however regex-shaped", () => {
    expect(
      isSearchablePattern({
        term: "cargo build (release",
        regex: false
      })
    ).toBe(true);
  });

  it("accepts a pattern that compiles", () => {
    expect(
      isSearchablePattern({
        term: "error:s+d+",
        regex: true
      })
    ).toBe(true);
  });

  it("rejects one that is still being typed", () => {
    expect(
      isSearchablePattern({
        term: "(unclosed",
        regex: true
      })
    ).toBe(false);
    expect(
      isSearchablePattern({
        term: "[a-",
        regex: true
      })
    ).toBe(false);
    expect(
      isSearchablePattern({
        term: "a**",
        regex: true
      })
    ).toBe(false);
  });
});
