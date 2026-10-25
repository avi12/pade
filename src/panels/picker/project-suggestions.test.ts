import { Key } from "@/lib/ui/keyboard/keys";
import {
  matchingProjects,
  MAX_PROJECT_SUGGESTIONS,
  suggestionKeyStep,
  SuggestionStepKind
} from "@/panels/picker/project-suggestions";
import { describe, expect, it } from "vitest";

const VISIBLE_THREE = {
  count: 3,
  isListVisible: true
};

const PADE = {
  path: "C:\\repositories\\avi\\pade",
  name: "pade"
};
const LABELLED = {
  path: "C:\\repositories\\temp\\swift-otter",
  name: "Invoice tool"
};
const NOTES = {
  path: "C:\\repositories\\avi\\notes",
  name: "notes"
};

describe("matchingProjects", () => {
  it("suggests nothing for a blank name", () => {
    expect(
      matchingProjects({
        options: [PADE],
        typed: "   "
      })
    ).toEqual([]);
  });

  it("matches the display name as a substring, ignoring case", () => {
    expect(
      matchingProjects({
        options: [PADE, LABELLED, NOTES],
        typed: "VOICE"
      })
    ).toEqual([LABELLED]);
  });

  it("matches the folder name when a label hides it", () => {
    expect(
      matchingProjects({
        options: [PADE, LABELLED],
        typed: "otter"
      })
    ).toEqual([LABELLED]);
  });

  it("ignores surrounding spaces in what was typed", () => {
    expect(
      matchingProjects({
        options: [PADE, NOTES],
        typed: " not "
      })
    ).toEqual([NOTES]);
  });

  it("keeps the given order, recents before the rest", () => {
    expect(
      matchingProjects({
        options: [NOTES, PADE],
        typed: "e"
      })
    ).toEqual([NOTES, PADE]);
  });

  it("never matches on the root above the folder", () => {
    expect(
      matchingProjects({
        options: [PADE],
        typed: "repositories"
      })
    ).toEqual([]);
  });

  it("caps the suggestions", () => {
    const options = Array.from({ length: MAX_PROJECT_SUGGESTIONS + 3 }, (_, i) => ({
      path: `C:\\code\\app-${i}`,
      name: `app-${i}`
    }));

    expect(
      matchingProjects({
        options,
        typed: "app"
      })
    ).toEqual(options.slice(0, MAX_PROJECT_SUGGESTIONS));
  });
});

describe("suggestionKeyStep", () => {
  it("highlights the first row on Down from none", () => {
    expect(
      suggestionKeyStep({
        ...VISIBLE_THREE,
        key: Key.ArrowDown,
        highlightedIndex: -1
      })
    ).toEqual({
      kind: SuggestionStepKind.Move,
      activeIndex: 0,
      isListOpen: true
    });
  });

  it("wraps Up from the first row to the last", () => {
    expect(
      suggestionKeyStep({
        ...VISIBLE_THREE,
        key: Key.ArrowUp,
        highlightedIndex: 0
      })
    ).toEqual({
      kind: SuggestionStepKind.Move,
      activeIndex: 2,
      isListOpen: true
    });
  });

  it("reopens a dismissed list on an arrow", () => {
    expect(
      suggestionKeyStep({
        count: 2,
        isListVisible: false,
        key: Key.ArrowDown,
        highlightedIndex: -1
      })
    ).toEqual({
      kind: SuggestionStepKind.Move,
      activeIndex: 0,
      isListOpen: true
    });
  });

  it("leaves arrows to the field when nothing matches", () => {
    expect(
      suggestionKeyStep({
        count: 0,
        isListVisible: false,
        key: Key.ArrowDown,
        highlightedIndex: -1
      })
    ).toBeNull();
  });

  it("picks the highlighted row on Enter", () => {
    expect(
      suggestionKeyStep({
        ...VISIBLE_THREE,
        key: Key.Enter,
        highlightedIndex: 1
      })
    ).toEqual({
      kind: SuggestionStepKind.Pick,
      index: 1
    });
  });

  it("leaves Enter to the form while nothing is highlighted", () => {
    expect(
      suggestionKeyStep({
        ...VISIBLE_THREE,
        key: Key.Enter,
        highlightedIndex: -1
      })
    ).toBeNull();
  });

  it("closes an open list on Escape", () => {
    expect(
      suggestionKeyStep({
        ...VISIBLE_THREE,
        key: Key.Escape,
        highlightedIndex: 1
      })
    ).toEqual({
      kind: SuggestionStepKind.Move,
      activeIndex: -1,
      isListOpen: false
    });
  });

  it("leaves Escape alone while the list is closed", () => {
    expect(
      suggestionKeyStep({
        count: 3,
        isListVisible: false,
        key: Key.Escape,
        highlightedIndex: -1
      })
    ).toBeNull();
  });

  it("leaves Home and End to the caret", () => {
    expect(
      suggestionKeyStep({
        ...VISIBLE_THREE,
        key: Key.Home,
        highlightedIndex: 1
      })
    ).toBeNull();
  });
});
