import { isDragOverElement } from "@/lib/drag-position";
import {
  afterEach,
  beforeEach,
  describe,
  expect,
  it,
  vi
} from "vitest";

function elementAt(rectangle: Pick<DOMRect, "left" | "top" | "width" | "height">) {
  const box = {
    ...rectangle,
    right: rectangle.left + rectangle.width,
    bottom: rectangle.top + rectangle.height
  };
  return { getBoundingClientRect: () => box };
}

describe("isDragOverElement", () => {
  beforeEach(() => {
    vi.stubGlobal("devicePixelRatio", 2);
  });

  afterEach(() => {
    vi.unstubAllGlobals();
  });

  const pane = elementAt({
    left: 100,
    top: 50,
    width: 200,
    height: 100
  });

  it("scales Tauri's physical position to CSS pixels before testing the rect", () => {
    expect(
      isDragOverElement({
        element: pane,
        position: {
          x: 400,
          y: 200
        }
      })
    ).toBe(true);
  });

  it("rejects a position outside the element", () => {
    expect(
      isDragOverElement({
        element: pane,
        position: {
          x: 100,
          y: 200
        }
      })
    ).toBe(false);
    expect(
      isDragOverElement({
        element: pane,
        position: {
          x: 400,
          y: 400
        }
      })
    ).toBe(false);
  });

  it("never matches an element that is not rendered", () => {
    const hidden = elementAt({
      left: 0,
      top: 0,
      width: 0,
      height: 0
    });
    expect(
      isDragOverElement({
        element: hidden,
        position: {
          x: 0,
          y: 0
        }
      })
    ).toBe(false);
  });
});
