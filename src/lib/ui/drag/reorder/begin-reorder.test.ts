// Characterizes the gesture `beginReorder` runs, with the DOM helpers stubbed so it
// needs no browser: what a press, a move, a release, an Escape and a drop-out each
// commit, report and clean up. Pins behaviour so the engine can be split safely.

import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import { beginReorder } from "@/lib/ui/drag/reorder/drag-reorder";
import type * as reorderGeometryModule from "@/lib/ui/drag/reorder/reorder-geometry";
import { Axis } from "@/lib/ui/drag/reorder/reorder-types";
import type { BeginReorderOptions } from "@/lib/ui/drag/reorder/reorder-types";

const SLOT_WIDTH = 100;
const SLOT_HEIGHT = 20;
const THRESHOLD = 5;

const mocks = vi.hoisted(() => {
  const hit: {
    isOutside: boolean;
    side: null | {
      id: string;
      side: string;
    };
  } = {
    isOutside: false,
    side: null
  };
  return {
    hit,
    shield: {
      remove: vi.fn()
    }
  };
});

vi.mock("svelte", () => ({ async tick() {} }));
vi.mock("@/lib/ui/motion", () => ({ prefersReducedMotion: () => true }));
vi.mock("@/lib/ui/drag/reorder/reorder-hit-test", () => ({
  hitElementAt: () => null,
  isOverOutsideZone: () => mocks.hit.isOutside,
  resolveDropTarget: () => mocks.hit.side
}));
vi.mock("@/lib/ui/drag/reorder/reorder-lift", () => ({
  SPRING_MILLISECONDS: 300,
  liftSiblings: vi.fn(),
  raiseShield: () => mocks.shield,
  unclipAncestors: () => [],
  reclipAncestors: vi.fn(),
  clearItemStyles: vi.fn(),
  springToTarget: vi.fn()
}));
vi.mock("@/lib/ui/drag/reorder/reorder-geometry", async importOriginal => {
  const original = await importOriginal<typeof reorderGeometryModule>();
  return {
    ...original,
    measureSiblings: ({ elParent, idAttribute }: { elParent: FakeElement; idAttribute: string }) =>
      elParent.children.map((elItem, i) => ({
        id: elItem.getAttribute(idAttribute) ?? "",
        elItem,
        left: i * SLOT_WIDTH,
        top: 0,
        width: SLOT_WIDTH,
        height: SLOT_HEIGHT
      }))
  };
});

class FakeElement {
  style: Record<string, string> = {};
  children: FakeElement[] = [];
  parentElement: FakeElement | null = null;
  constructor(private readonly attributes: Record<string, string> = {}) {}
  getAttribute(name: string) {
    return this.attributes[name] ?? null;
  }
  closest() {
    return this;
  }
}

type Listener = (e: unknown) => void;
const listeners = new Map<string, Set<Listener>>();

function fire({ type, detail }: { type: string; detail: Record<string, unknown> }) {
  for (const listener of listeners.get(type) ?? []) {
    listener(detail);
  }
}

function pointer({ type, x }: { type: string; x: number }) {
  fire({
    type,
    detail: {
      clientX: x,
      clientY: 0
    }
  });
}

function isReorderOptions(value: unknown): value is BeginReorderOptions {
  return typeof value === "object" && value !== null && "e" in value;
}

function reorderOptions(value: unknown) {
  if (!isReorderOptions(value)) {
    throw new TypeError("not reorder options");
  }

  return value;
}

function setUp({ button = 0 } = {}) {
  const elParent = new FakeElement();
  const elItems = ["a", "b", "c"].map(id => new FakeElement({ "data-id": id }));
  for (const elItem of elItems) {
    elItem.parentElement = elParent;
  }
  elParent.children = elItems;
  const onCommit = vi.fn();
  const onHint = vi.fn();
  const onDropOutside = vi.fn();
  const options = reorderOptions({
    e: {
      button,
      target: elItems[0],
      clientX: 10,
      clientY: 0
    },
    itemSelector: ".item",
    idAttribute: "data-id",
    axis: Axis.HORIZONTAL,
    onCommit,
    onHint,
    onDropOutside,
    outsideSelector: ".outside"
  });
  return {
    options,
    elItems,
    onCommit,
    onHint,
    onDropOutside
  };
}

function flush() {
  return new Promise<void>(resolve => setTimeout(resolve, 0));
}

beforeEach(() => {
  listeners.clear();
  mocks.hit.isOutside = false;
  mocks.hit.side = null;
  vi.stubGlobal("Element", FakeElement);
  vi.stubGlobal("document", {
    body: {
      style: {
        userSelect: "auto"
      }
    }
  });
  vi.stubGlobal("addEventListener", (type: string, listener: Listener) => {
    listeners.set(type, (listeners.get(type) ?? new Set()).add(listener));
  });
  vi.stubGlobal("removeEventListener", (type: string, listener: Listener) => {
    listeners.get(type)?.delete(listener);
  });
});

afterEach(async () => {
  pointer({
    type: "pointercancel",
    x: 0
  });
  await flush();
  vi.unstubAllGlobals();
});

describe("beginReorder", () => {
  it("commits nothing and reports nothing for a press that never moved", async () => {
    const { options, onCommit, onHint } = setUp();
    beginReorder(options);
    pointer({
      type: "pointerup",
      x: 10
    });
    await flush();

    expect(onCommit).not.toHaveBeenCalled();
    expect(onHint).not.toHaveBeenCalled();
    expect(listeners.get("pointermove")?.size).toBe(0);
  });

  it("ignores a second press while a drag is live", () => {
    const first = setUp();
    beginReorder(first.options);
    const second = setUp();
    beginReorder(second.options);

    expect(listeners.get("pointermove")?.size).toBe(1);
  });

  it("ignores a non-primary press", () => {
    const { options } = setUp({ button: 1 });
    beginReorder(options);

    expect(listeners.get("pointermove")?.size ?? 0).toBe(0);
  });

  it("slides the dragged item with the pointer and reports a hint", () => {
    const { options, elItems, onHint } = setUp();
    beginReorder(options);
    pointer({
      type: "pointermove",
      x: 10 + THRESHOLD + 40
    });

    expect(elItems[0].style.translate).toBe("45px 0px");
    expect(onHint).toHaveBeenLastCalledWith({
      id: "a",
      isOutside: false,
      dropTarget: null,
      pointerX: 55,
      pointerY: 0
    });
  });

  it("commits the moved order after a drop past a neighbour", async () => {
    const { options, onCommit, onHint } = setUp();
    beginReorder(options);
    pointer({
      type: "pointermove",
      x: 10 + 130
    });
    pointer({
      type: "pointerup",
      x: 140
    });
    await flush();

    expect(onCommit).toHaveBeenCalledWith(["b", "a", "c"]);
    expect(onHint).toHaveBeenLastCalledWith(null);
  });

  it("commits the unchanged order for a drop back in the same slot", async () => {
    const { options, onCommit } = setUp();
    beginReorder(options);
    pointer({
      type: "pointermove",
      x: 20
    });
    pointer({
      type: "pointerup",
      x: 20
    });
    await flush();

    expect(onCommit).toHaveBeenCalledWith(["a", "b", "c"]);
  });

  it("commits nothing on Escape and clears the hint", async () => {
    const { options, onCommit, onHint } = setUp();
    beginReorder(options);
    pointer({
      type: "pointermove",
      x: 140
    });
    fire({
      type: "keydown",
      detail: {
        key: "Escape",
        preventDefault: vi.fn()
      }
    });
    await flush();

    expect(onCommit).not.toHaveBeenCalled();
    expect(onHint).toHaveBeenLastCalledWith(null);
  });

  it("hands a release over the outside zone to onDropOutside, never onCommit", async () => {
    const { options, onCommit, onDropOutside, onHint } = setUp();
    beginReorder(options);
    mocks.hit.isOutside = true;
    mocks.hit.side = {
      id: "pane",
      side: "left"
    };
    pointer({
      type: "pointermove",
      x: 60
    });
    pointer({
      type: "pointerup",
      x: 60
    });
    await flush();

    expect(onCommit).not.toHaveBeenCalled();
    expect(onDropOutside).toHaveBeenCalledWith({
      id: "a",
      dropTarget: {
        id: "pane",
        side: "left"
      },
      pointerX: 60,
      pointerY: 0
    });
    expect(onHint).toHaveBeenLastCalledWith(null);
  });

  it("allows a new drag once the previous one has settled", async () => {
    const first = setUp();
    beginReorder(first.options);
    pointer({
      type: "pointermove",
      x: 140
    });
    pointer({
      type: "pointerup",
      x: 140
    });
    await flush();
    const second = setUp();
    beginReorder(second.options);

    expect(listeners.get("pointermove")?.size).toBe(1);
  });
});
