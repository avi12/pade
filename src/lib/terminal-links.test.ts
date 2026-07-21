import { computeLinks } from "@/lib/terminal-links";
import { describe, expect, it } from "vitest";

// A tiny stand-in for xterm's buffer: each row is a plain string plus an
// optional soft-wrap flag. `getCell` mirrors the real API — a column past the
// row's text reads back empty, exactly like an unwritten cell.
interface MockRow {
  text: string;
  wrapped?: boolean;
}

function makeTerminal({ rows, columns }: {
  rows: MockRow[];
  columns: number;
}) {
  function getLine(y: number) {
    const row = rows[y];
    if (!row) {
      return undefined;
    }

    return {
      isWrapped: row.wrapped ?? false,
      length: columns,
      getCell(column: number) {
        return {
          getChars: () => row.text[column] ?? ""
        };
      }
    };
  }

  return {
    cols: columns,
    buffer: {
      active: {
        getLine
      }
    }
  };
}

describe("computeLinks", () => {
  it("finds a URL sitting on a single line", () => {
    const url = "https://example.com/page";
    const links = computeLinks({
      terminal: makeTerminal({
        rows: [{ text: `See ${url} here` }],
        columns: 40
      }),
      bufferLineNumber: 1,
      openUrl() {}
    });

    expect(links).toHaveLength(1);
    expect(links[0].text).toBe(url);
    expect(links[0].range).toEqual({
      start: {
        x: 5,
        y: 1
      },
      end: {
        x: 28,
        y: 1
      }
    });
  });

  it("rejoins a URL the terminal soft-wrapped onto column 0", () => {
    const url = "https://example.com/verylongpath";
    const links = computeLinks({
      terminal: makeTerminal({
        rows: [
          { text: url.slice(0, 20) },
          {
            text: url.slice(20),
            wrapped: true
          }
        ],
        columns: 20
      }),
      bufferLineNumber: 1,
      openUrl() {}
    });

    expect(links).toHaveLength(1);
    expect(links[0].text).toBe(url);
    expect(links[0].range).toEqual({
      start: {
        x: 1,
        y: 1
      },
      end: {
        x: 12,
        y: 2
      }
    });
  });

  it("rejoins a URL hard-wrapped onto column 0 (upper row filled to the edge)", () => {
    const url = "https://example.com/verylongpath";
    const links = computeLinks({
      terminal: makeTerminal({
        rows: [
          { text: url.slice(0, 20) },
          { text: url.slice(20) }
        ],
        columns: 20
      }),
      bufferLineNumber: 1,
      openUrl() {}
    });

    expect(links).toHaveLength(1);
    expect(links[0].text).toBe(url);
    expect(links[0].range).toEqual({
      start: {
        x: 1,
        y: 1
      },
      end: {
        x: 12,
        y: 2
      }
    });
  });

  it("rejoins Claude's self-wrapped URL across an indented, right-margined row", () => {
    const url = "https://example.com/verylongpath";
    // Upper row stops a column shy of the edge (a right margin); the
    // continuation is indented two columns under Claude's text block.
    const links = computeLinks({
      terminal: makeTerminal({
        rows: [
          { text: "https://example.com" },
          { text: "  /verylongpath" }
        ],
        columns: 20
      }),
      bufferLineNumber: 2,
      openUrl() {}
    });

    expect(links).toHaveLength(1);
    expect(links[0].text).toBe(url);
    expect(links[0].range).toEqual({
      start: {
        x: 1,
        y: 1
      },
      end: {
        x: 15,
        y: 2
      }
    });
  });

  it("stops a full-width URL short of the box-drawing rule below it", () => {
    const url = "https://api.ezcount.co.il/paypal/ipn/7f1f48eb4da946e63704c8a3";
    const links = computeLinks({
      terminal: makeTerminal({
        rows: [
          { text: url },
          { text: "─".repeat(url.length) }
        ],
        columns: url.length
      }),
      bufferLineNumber: 1,
      openUrl() {}
    });

    expect(links).toHaveLength(1);
    expect(links[0].text).toBe(url);
    expect(links[0].range.end.y).toBe(1);
    expect(links[0].range.end.x).toBe(url.length);
  });

  it("joins two full prose rows without inventing a link", () => {
    const links = computeLinks({
      terminal: makeTerminal({
        rows: [
          { text: "the quick brown fox " },
          { text: "jumps the lazy dogs!" }
        ],
        columns: 20
      }),
      bufferLineNumber: 1,
      openUrl() {}
    });

    expect(links).toHaveLength(0);
  });
});
