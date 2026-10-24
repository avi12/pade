import { nextSidebarPosition, SidebarKey } from "@/lib/shell/sidebar/sidebar-keyboard";
import { describe, expect, it } from "vitest";

// A project header with + and options, a live chat with its stop button, a
// recorded conversation, then the next project's header.
const LINE_LENGTHS = [3, 2, 1, 3];

function move(key: string, line: number, item: number) {
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
    expect(move(SidebarKey.Right, 0, 0)).toEqual({
      line: 0,
      item: 1
    });
    expect(move(SidebarKey.Right, 0, 1)).toEqual({
      line: 0,
      item: 2
    });
    expect(move(SidebarKey.Right, 0, 2)).toBeNull();
    expect(move(SidebarKey.Left, 0, 2)).toEqual({
      line: 0,
      item: 1
    });
    expect(move(SidebarKey.Left, 0, 0)).toBeNull();
  });

  it("goes down through the conversations onto the next header, landing on each main control", () => {
    expect(move(SidebarKey.Down, 0, 2)).toEqual({
      line: 1,
      item: 0
    });
    expect(move(SidebarKey.Down, 2, 0)).toEqual({
      line: 3,
      item: 0
    });
    expect(move(SidebarKey.Down, 3, 0)).toBeNull();
    expect(move(SidebarKey.Up, 1, 1)).toEqual({
      line: 0,
      item: 0
    });
    expect(move(SidebarKey.Up, 0, 0)).toBeNull();
  });

  it("jumps to the first and last line", () => {
    expect(move(SidebarKey.Home, 2, 0)).toEqual({
      line: 0,
      item: 0
    });
    expect(move(SidebarKey.End, 0, 1)).toEqual({
      line: 3,
      item: 0
    });
  });

  it("ignores other keys", () => {
    expect(move("Enter", 0, 0)).toBeNull();
  });
});
