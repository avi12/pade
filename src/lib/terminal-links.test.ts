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

  it("rejoins a wrapped URL inside a pasted block's left-bar frame", () => {
    // Claude's TUI draws a `▌` bar down the left edge of a pasted block. Two
    // pasted URLs: the second wraps, its continuation row starting with the bar
    // again. The bar and its padding must not leak into the joined text, or the
    // second URL's match dies at the row boundary.
    const first = "https://developer.chrome.com/docs/webstore/program_policies/#limited_use";
    const second = "https://developer.chrome.com/webstore/program_policies";
    const upper = `▌ ${first} ${second.slice(0, 20)}`;
    const links = computeLinks({
      terminal: makeTerminal({
        rows: [
          { text: upper },
          { text: `▌ ${second.slice(20)}` }
        ],
        columns: upper.length
      }),
      bufferLineNumber: 1,
      openUrl() {}
    });

    expect(links).toHaveLength(2);
    expect(links[0].text).toBe(first);
    expect(links[1].text).toBe(second);
    expect(links[1].range.end.y).toBe(2);
  });

  it("keeps two rule-padded tool-call rows as separate links", () => {
    // Claude underlines a tool call by padding the row to the edge with `─`.
    // Two stacked WebFetch rows must stay two links — the rule reaching the
    // edge is not a wrap, and the `%` opening the lower row must not be glued
    // onto the upper URL.
    const first = "https://developer.chrome.com/docs/webstore/program_policies/#limited_use";
    const second = "https://developer.chrome.com/webstore/program_policies";
    const upper = `% WebFetch ${first} `;
    const columns = upper.length + 6;
    const links = computeLinks({
      terminal: makeTerminal({
        rows: [
          { text: upper + "─".repeat(columns - upper.length) },
          { text: `% WebFetch ${second}` }
        ],
        columns
      }),
      bufferLineNumber: 1,
      openUrl() {}
    });

    expect(links).toHaveLength(1);
    expect(links[0].text).toBe(first);
    expect(links[0].range.end.y).toBe(1);
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
