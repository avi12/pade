import { beginTabDrag } from "@/lib/session/tabs/tab-drag";
import { Axis } from "@/lib/ui/drag/drag-reorder";
import type { BeginReorderOptions } from "@/lib/ui/drag/drag-reorder";
import { testStandIn } from "@/lib/util/test-stand-in";
import {
  beforeEach,
  describe,
  expect,
  it,
  vi
} from "vitest";

const mocks = vi.hoisted(() => ({
  beginReorder: vi.fn<(options: BeginReorderOptions) => void>()
}));

vi.mock("@/lib/ui/drag/drag-reorder", async importOriginal => ({
  ...await importOriginal<typeof import("@/lib/ui/drag/drag-reorder")>(),
  beginReorder: mocks.beginReorder
}));

const PRESS = testStandIn<PointerEvent>({ type: "pointerdown" });
const DROP = {
  id: "session-a",
  dropTarget: null,
  pointerX: 10,
  pointerY: 20
};

function startedOptions() {
  const options = mocks.beginReorder.mock.calls[0]?.[0];
  if (options === undefined) {
    throw new Error("beginReorder was not called");
  }

  return options;
}

describe("beginTabDrag", () => {
  beforeEach(() => {
    mocks.beginReorder.mockClear();
  });

  it("drags the strip's pills horizontally, leaving their controls alone", () => {
    beginTabDrag({
      e: PRESS,
      isRenaming: false
    });

    expect(startedOptions()).toMatchObject({
      e: PRESS,
      itemSelector: "[data-session-tab]",
      idAttribute: "data-session-tab",
      axis: Axis.Horizontal,
      threshold: 5,
      ignoreSelector: "[data-noreorder]",
      outsideSelector: "[data-panes]"
    });
  });

  it("routes a reorder, a live hint and a drop on the panes to their callbacks", () => {
    const onreorder = vi.fn<(orderedIds: string[]) => void>();
    const ondraghint = vi.fn();
    const onsplit = vi.fn();
    beginTabDrag({
      e: PRESS,
      isRenaming: false,
      onreorder,
      ondraghint,
      onsplit
    });

    const options = startedOptions();
    options.onCommit(["b", "a"]);
    options.onHint?.(null);
    options.onDropOutside?.(DROP);

    expect(onreorder).toHaveBeenCalledWith(["b", "a"]);
    expect(ondraghint).toHaveBeenCalledWith(null);
    expect(onsplit).toHaveBeenCalledWith(DROP);
  });

  it("tolerates a strip that listens to none of the drag's outcomes", () => {
    beginTabDrag({
      e: PRESS,
      isRenaming: false
    });

    const options = startedOptions();

    expect(() => {
      options.onCommit(["a"]);
      options.onHint?.(null);
      options.onDropOutside?.(DROP);
    }).not.toThrow();
  });

  it("never starts a drag while a pill is being renamed", () => {
    beginTabDrag({
      e: PRESS,
      isRenaming: true
    });

    expect(mocks.beginReorder).not.toHaveBeenCalled();
  });
});
