import {
  DropSide,
  insertionIndex,
  paneDropSide,
  paneInsertIndex,
  reorderedIds
} from "@/lib/reorder";
import { describe, expect, it } from "vitest";

const IDS = ["a", "b", "c", "d"];

describe("reorderedIds", () => {
  it("is a no-op when fromIndex === toIndex", () => {
    const order = reorderedIds({
      ids: IDS,
      fromIndex: 2,
      toIndex: 2
    });

    expect(order).toEqual(IDS);
  });

  it("moves an item left (toIndex counts the remaining items)", () => {
    // Lift "c" (index 2), drop it at index 1 among the rest [a, b, d].
    const order = reorderedIds({
      ids: IDS,
      fromIndex: 2,
      toIndex: 1
    });

    expect(order).toEqual(["a", "c", "b", "d"]);
  });

  it("moves an item right", () => {
    // Lift "a" (index 0); rest is [b, c, d], insert at index 2.
    const order = reorderedIds({
      ids: IDS,
      fromIndex: 0,
      toIndex: 2
    });

    expect(order).toEqual(["b", "c", "a", "d"]);
  });

  it("moves an item to the end", () => {
    const order = reorderedIds({
      ids: IDS,
      fromIndex: 0,
      toIndex: 3
    });

    expect(order).toEqual(["b", "c", "d", "a"]);
  });
});

describe("insertionIndex", () => {
  // Four items centered at 50 / 150 / 250 / 350 along the axis.
  const centers = [50, 150, 250, 350];

  it("keeps the index put while the dragged center stays over its own slot", () => {
    // Dragging index 1 (center 150), barely moved → no other center is passed.
    const index = insertionIndex({
      centers,
      fromIndex: 1,
      draggedCenter: 150
    });

    expect(index).toBe(1);
  });

  it("counts every other center left of the projected center (moving right)", () => {
    // Dragging index 0 past the centers at 150 and 250 (but not 350).
    const index = insertionIndex({
      centers,
      fromIndex: 0,
      draggedCenter: 260
    });

    expect(index).toBe(2);
  });

  it("drops to the front when the projected center passes nothing (moving left)", () => {
    // Dragging index 3 back before every other center.
    const index = insertionIndex({
      centers,
      fromIndex: 3,
      draggedCenter: 10
    });

    expect(index).toBe(0);
  });

  it("holds a swapped index until the pointer crosses the neighbour's new center (sticky drag)", () => {
    // The engine excludes the dragged item's *current* gap each move (passing it as
    // `fromIndex`), which is what makes a swap sticky rather than flickery.
    // "a" (index 0) dragged right past "b"'s center (150) → lands at index 1.
    expect(
      insertionIndex({
        centers,
        fromIndex: 0,
        draggedCenter: 160
      })
    ).toBe(1);

    // Now sitting in gap 1 (exclude index 1): easing back toward "b"'s OLD center
    // (150) does NOT snap back — the index holds at 1.
    expect(
      insertionIndex({
        centers,
        fromIndex: 1,
        draggedCenter: 140
      })
    ).toBe(1);

    // It only returns to 0 once the pointer crosses "b"'s NEW center (slot 0 = 50).
    expect(
      insertionIndex({
        centers,
        fromIndex: 1,
        draggedCenter: 40
      })
    ).toBe(0);
  });
});

describe("paneDropSide", () => {
  // A pane spanning x ∈ [100, 300), so its midpoint sits at x = 200.
  const pane = {
    left: 100,
    width: 200
  };

  it("reads the left half before the midpoint", () => {
    expect(
      paneDropSide({
        pointerX: 140,
        ...pane
      })
    ).toBe(DropSide.left);
  });

  it("reads the right half at or past the midpoint", () => {
    expect(
      paneDropSide({
        pointerX: 260,
        ...pane
      })
    ).toBe(DropSide.right);
  });

  it("treats the exact midpoint as the right half", () => {
    expect(
      paneDropSide({
        pointerX: 200,
        ...pane
      })
    ).toBe(DropSide.right);
  });
});

describe("paneInsertIndex", () => {
  it("lands after the target on the right side", () => {
    // Dragging a new tab "x" onto "b"'s right half of panes [a, b, c].
    const insertAt = paneInsertIndex({
      paneIds: ["a", "b", "c"],
      draggedId: "x",
      targetId: "b",
      side: DropSide.right
    });

    expect(insertAt).toBe(2);
  });

  it("lands before the target on the left side", () => {
    const insertAt = paneInsertIndex({
      paneIds: ["a", "b", "c"],
      draggedId: "x",
      targetId: "b",
      side: DropSide.left
    });

    expect(insertAt).toBe(1);
  });

  it("places a session not currently in paneIds by the target (nothing filtered out)", () => {
    // "x" isn't shown, so base === paneIds; dropping on "a"'s left → index 0.
    const insertAt = paneInsertIndex({
      paneIds: ["a", "b"],
      draggedId: "x",
      targetId: "a",
      side: DropSide.left
    });

    expect(insertAt).toBe(0);
  });

  it("appends onto the single shown pane (single-pane edge)", () => {
    // Panes = [a]; drop a new "b" onto "a"'s right half → base [a], index 1.
    const insertAt = paneInsertIndex({
      paneIds: ["a"],
      draggedId: "b",
      targetId: "a",
      side: DropSide.right
    });

    expect(insertAt).toBe(1);
  });

  it("repositions a dragged id already present (filtered out of the base first)", () => {
    // "a" is already shown; re-dropping it onto "c"'s right side. base = [b, c],
    // targetIndex(c) = 1, right → 2 → App builds [b, c, a].
    const paneIds = ["a", "b", "c"];
    const insertAt = paneInsertIndex({
      paneIds,
      draggedId: "a",
      targetId: "c",
      side: DropSide.right
    });
    expect(insertAt).toBe(2);

    const base = paneIds.filter(id => id !== "a");
    const rebuilt = [...base.slice(0, insertAt), "a", ...base.slice(insertAt)];
    expect(rebuilt).toEqual(["b", "c", "a"]);
  });

  it("appends when the target isn't in the base list", () => {
    // Target got filtered out (it was the dragged id) or is unknown → append.
    const insertAt = paneInsertIndex({
      paneIds: ["a", "b"],
      draggedId: "a",
      targetId: "a",
      side: DropSide.left
    });

    expect(insertAt).toBe(1);
  });
});
