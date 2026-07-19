// The drag engine (`drag-reorder.ts`) is DOM-driven, but the decision it makes on
// release — what order to commit — is the pure `committedOrderOnDrop` in
// `@/lib/reorder`. These cover the regression it fixes: a below-threshold /
// same-slot drop must commit the UNCHANGED full order (never a shorter list, never
// nothing), so the caller's session list can't drift out of sync with the DOM the
// engine moved and later drop the "dropped-in-place" tab.

import { committedOrderOnDrop } from "@/lib/reorder";
import { describe, expect, it } from "vitest";

describe("committedOrderOnDrop", () => {
  const ids = ["a", "b", "c"];

  it("commits the full unchanged order for a same-slot drop (never a shorter list)", () => {
    const order = committedOrderOnDrop({
      ids,
      fromIndex: 1,
      toIndex: 1,
      cancelled: false
    });

    expect(order).toEqual(ids);
    expect(order).toHaveLength(ids.length);
  });

  it("commits every id when the last pill is dropped back onto itself", () => {
    // The reported repro: the second (last) of two tabs, released in place.
    const twoTabs = ["first", "second"];
    const order = committedOrderOnDrop({
      ids: twoTabs,
      fromIndex: 1,
      toIndex: 1,
      cancelled: false
    });

    expect(order).toEqual(twoTabs);
  });

  it("commits the moved order, still full length, when the slot changed", () => {
    const order = committedOrderOnDrop({
      ids,
      fromIndex: 0,
      toIndex: 2,
      cancelled: false
    });

    expect(order).toEqual(["b", "c", "a"]);
    expect(order).toHaveLength(ids.length);
  });

  it("commits nothing on a cancel (Escape / pointercancel)", () => {
    const order = committedOrderOnDrop({
      ids,
      fromIndex: 1,
      toIndex: 0,
      cancelled: true
    });

    expect(order).toBeNull();
  });

  it("never yields a list shorter than the input for any in-range slot", () => {
    for (let fromIndex = 0; fromIndex < ids.length; fromIndex++) {
      for (let toIndex = 0; toIndex < ids.length; toIndex++) {
        const order = committedOrderOnDrop({
          ids,
          fromIndex,
          toIndex,
          cancelled: false
        });

        expect(order).not.toBeNull();
        expect(order).toHaveLength(ids.length);
        expect([...(order ?? [])].sort()).toEqual([...ids].sort());
      }
    }
  });
});
