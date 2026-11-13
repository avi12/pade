import { describe, expect, it } from "vitest";

import { nextSidebarPosition, SidebarKey } from "@/lib/shell/sidebar/projects/sidebar-keyboard";

// A project header with + and options, a live chat with its stop button, a
// recorded conversation, then the next project's header.
const LINE_LENGTHS = [3, 2, 1, 3];

function move({ key, line, item }: { key: string; line: number; item: number }) {
  return nextSidebarPosition({
    key,
    position: {
      line,
      item
    },
    lineLengths: LINE_LENGTHS
  });
}

describe("nextSidebarPosition", () => {
  it("walks along a header to its buttons and back", () => {
    expect(
      move({
        key: SidebarKey.RIGHT,
        line: 0,
        item: 0
      })
    ).toEqual({
      line: 0,
      item: 1
    });
    expect(
      move({
        key: SidebarKey.RIGHT,
        line: 0,
        item: 1
      })
    ).toEqual({
      line: 0,
      item: 2
    });
    expect(
      move({
        key: SidebarKey.RIGHT,
        line: 0,
        item: 2
      })
    ).toBeNull();
    expect(
      move({
        key: SidebarKey.LEFT,
        line: 0,
        item: 2
      })
    ).toEqual({
      line: 0,
      item: 1
    });
    expect(
      move({
        key: SidebarKey.LEFT,
        line: 0,
        item: 0
      })
    ).toBeNull();
  });

  it("goes down through the conversations onto the next header, landing on each main control", () => {
    expect(
      move({
        key: SidebarKey.DOWN,
        line: 0,
        item: 2
      })
    ).toEqual({
      line: 1,
      item: 0
    });
    expect(
      move({
        key: SidebarKey.DOWN,
        line: 2,
        item: 0
      })
    ).toEqual({
      line: 3,
      item: 0
    });
    expect(
      move({
        key: SidebarKey.DOWN,
        line: 3,
        item: 0
      })
    ).toBeNull();
    expect(
      move({
        key: SidebarKey.UP,
        line: 1,
        item: 1
      })
    ).toEqual({
      line: 0,
      item: 0
    });
    expect(
      move({
        key: SidebarKey.UP,
        line: 0,
        item: 0
      })
    ).toBeNull();
  });

  it("jumps to the first and last line", () => {
    expect(
      move({
        key: SidebarKey.HOME,
        line: 2,
        item: 0
      })
    ).toEqual({
      line: 0,
      item: 0
    });
    expect(
      move({
        key: SidebarKey.END,
        line: 0,
        item: 1
      })
    ).toEqual({
      line: 3,
      item: 0
    });
  });

  it("ignores other keys", () => {
    expect(
      move({
        key: "Enter",
        line: 0,
        item: 0
      })
    ).toBeNull();
  });
});
