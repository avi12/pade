import { nextPaneId, paneIdAt, previousPaneId } from "@/lib/pane-nav";
import { describe, expect, it } from "vitest";

const panes = ["a", "b", "c", "d"];

describe("paneIdAt", () => {
  it("returns the id at a 0-based position", () => {
    expect(
      paneIdAt({
        paneIds: panes,
        index: 0
      })
    ).toBe("a");
    expect(
      paneIdAt({
        paneIds: panes,
        index: 2
      })
    ).toBe("c");
    expect(
      paneIdAt({
        paneIds: panes,
        index: 3
      })
    ).toBe("d");
  });

  it("is null past the last pane, for a negative index, or on an empty split", () => {
    expect(
      paneIdAt({
        paneIds: panes,
        index: 4
      })
    ).toBeNull();
    expect(
      paneIdAt({
        paneIds: panes,
        index: -1
      })
    ).toBeNull();
    expect(
      paneIdAt({
        paneIds: [],
        index: 0
      })
    ).toBeNull();
  });
});

describe("previousPaneId", () => {
  it("steps one pane to the left", () => {
    expect(
      previousPaneId({
        paneIds: panes,
        activeId: "c"
      })
    ).toBe("b");
    expect(
      previousPaneId({
        paneIds: panes,
        activeId: "b"
      })
    ).toBe("a");
  });

  it("wraps from the first pane round to the last", () => {
    expect(
      previousPaneId({
        paneIds: panes,
        activeId: "a"
      })
    ).toBe("d");
  });

  it("stays put with a single pane", () => {
    expect(
      previousPaneId({
        paneIds: ["only"],
        activeId: "only"
      })
    ).toBe("only");
  });

  it("is null when the active id isn't shown or the split is empty", () => {
    expect(
      previousPaneId({
        paneIds: panes,
        activeId: "z"
      })
    ).toBeNull();
    expect(
      previousPaneId({
        paneIds: [],
        activeId: "a"
      })
    ).toBeNull();
  });
});

describe("nextPaneId", () => {
  it("steps one pane to the right", () => {
    expect(
      nextPaneId({
        paneIds: panes,
        activeId: "a"
      })
    ).toBe("b");
    expect(
      nextPaneId({
        paneIds: panes,
        activeId: "c"
      })
    ).toBe("d");
  });

  it("wraps from the last pane round to the first", () => {
    expect(
      nextPaneId({
        paneIds: panes,
        activeId: "d"
      })
    ).toBe("a");
  });

  it("stays put with a single pane", () => {
    expect(
      nextPaneId({
        paneIds: ["only"],
        activeId: "only"
      })
    ).toBe("only");
  });

  it("is null when the active id isn't shown or the split is empty", () => {
    expect(
      nextPaneId({
        paneIds: panes,
        activeId: "z"
      })
    ).toBeNull();
    expect(
      nextPaneId({
        paneIds: [],
        activeId: "a"
      })
    ).toBeNull();
  });
});
