import { computeLinks, registerWrappedLinkProvider } from "@/lib/terminal-links";
import type { ILink } from "@xterm/xterm";
import { describe, expect, it } from "vitest";

// The `computeLinks`-shaped bits a registration mock needs so it structurally
// satisfies the exported parameter without a type assertion.
const emptyBuffer = {
  cols: 80,
  rows: 24,
  element: undefined,
  buffer: {
    active: {
      getLine: () => undefined,
      viewportY: 0
    }
  }
};

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

// A wrapped URL is returned as one link per row; each must stay on a single row
// so xterm never treats an intermediate cell (padding) as part of the link.
function expectEverySpanSingleRow(links: ILink[]): void {
  for (const link of links) {
    expect(link.range.start.y).toBe(link.range.end.y);
  }
}

function textsOf(links: ILink[]): string[] {
  return links.map(link => link.text);
}

// A spread of URL shapes to split at every wrap position: schemes, host-only,
// localhost, IPv6, bare IP, userinfo, ports, query, fragment, dotted paths, and
// — the adversarial ones — percent-escapes (`%XX`), so a wrap that lands mid-
// escape (`…%` | `2Fpath`, `…%2` | `Fpath`) is exercised too.
const URL_CORPUS = [
  "https://example.com",
  "http://localhost",
  "http://localhost:3000/dashboard",
  "https://[::1]:8080/api/health",
  "http://192.168.1.10/admin/settings?tab=security#top",
  "https://user:pass@host.example.com:9443/path/to/resource",
  "https://community.getro.com/companies/greylock-2/jobs/74075143-applied-ai",
  "https://job-boards.greenhouse.io/neosecurityinc/jobs/4323679009",
  "https://sub.domain.co.uk/a.b.c/d?e=f&h=i",
  "https://example.com/search?q=hello%20world&lang=en%2Dus",
  "https://example.com/path/%E2%9C%93/check%20mark%21",
  "https://api.example.com/v1/items/42%2Fnested%3Fkey%3Dvalue",
  "https://example.com/feed?type=rss&sort=new&page=2",
  // Ends on `&` (a valid URL byte the pattern keeps) — every split of it,
  // including one that leaves the trailing `&` on its own row, must still stitch.
  "https://example.com/list?a=1&b=2&"
];

// Every link opens the whole URL, each span stays on its own row, and the spans
// together cover exactly the URL's glyph count — no padding cell, no dropped
// glyph — regardless of where the wrap fell.
function expectRecoversWholeUrl({ links, url, rows, label }: {
  links: ILink[];
  url: string;
  rows: number[];
  label: string;
}): void {
  expect(new Set(textsOf(links)), label).toEqual(new Set([url]));
  const coveredRows = [...new Set(links.map(link => link.range.start.y))]
    .sort((first, second) => first - second);
  expect(coveredRows, label).toEqual(rows);
  expectEverySpanSingleRow(links);
  const covered = links.reduce((sum, link) => sum + (link.range.end.x - link.range.start.x + 1), 0);
  expect(covered, label).toBe(url.length);
}

describe("computeLinks URL-shape robustness (property)", () => {
  it("recovers every corpus URL sitting on a single line", () => {
    for (const url of URL_CORPUS) {
      const [link] = computeLinks({
        terminal: makeTerminal({
          rows: [{ text: `See ${url} !` }],
          columns: url.length + 20
        }),
        bufferLineNumber: 1,
        openUrl() {}
      });
      // If this fails, the corpus entry itself isn't a shape the ported pattern
      // matches whole (fix the corpus, not the wrap logic).
      expect(link?.text, url).toBe(url);
    }
  });

  it("recovers a self-wrapped URL split at every position, from either row", () => {
    const indent = "   ";
    for (const url of URL_CORPUS) {
      // A 1-char continuation is below the min-continuation guard by design, so
      // stop at length-2; every earlier split leaves ≥2 glyphs on the lower row.
      for (let split = 1; split <= url.length - 2; split += 1) {
        const upper = `4. Item (${url.slice(0, split)}`;
        const lower = `${indent}${url.slice(split)})`;
        // Wide terminal → the upper row ends mid-URL far short of the edge with
        // blank padding after it: the self-wrap path, not a soft/hard wrap.
        const columns = Math.max(upper.length, lower.length) + 12;
        const rowset = { rows: [{ text: upper }, { text: lower }], columns };

        for (const bufferLineNumber of [1, 2]) {
          expectRecoversWholeUrl({
            links: computeLinks({
              terminal: makeTerminal(rowset),
              bufferLineNumber,
              openUrl() {}
            }),
            url,
            rows: [1, 2],
            label: `url=${url} split=${split} anchor=${bufferLineNumber}`
          });
        }
      }
    }
  });

  it("recovers a URL self-wrapped across three rows, hovering any of them", () => {
    const url = "https://api.example.com/v1/items/42%2Fnested%3Fkey%3Dvalue";
    // The first break stays past the scheme (`https://`, 8 chars) — a real wrap
    // width is 40–120 columns, so the scheme is never split across rows; only a
    // sub-4-column width could fragment it, which no agent produces.
    for (let first = 10; first <= url.length - 6; first += 6) {
      for (let second = first + 3; second <= url.length - 2; second += 6) {
        const rows = [
          { text: `- (${url.slice(0, first)}` },
          { text: `  ${url.slice(first, second)}` },
          { text: `  ${url.slice(second)})` }
        ];
        const columns = Math.max(...rows.map(row => row.text.length)) + 10;
        // Hovering the top (scheme), middle, or bottom row must recover the whole
        // URL — the upward walk reaches the scheme past the scheme-less middle.
        for (const bufferLineNumber of [1, 2, 3]) {
          expectRecoversWholeUrl({
            links: computeLinks({
              terminal: makeTerminal({
                rows,
                columns
              }),
              bufferLineNumber,
              openUrl() {}
            }),
            url,
            rows: [1, 2, 3],
            label: `url=${url} first=${first} second=${second} anchor=${bufferLineNumber}`
          });
        }
      }
    }
  });
});

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

  it("rejoins a URL the terminal soft-wrapped onto column 0, as one span per row", () => {
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

    // One link per row; both open the whole URL, neither range crosses rows.
    expect(links).toHaveLength(2);
    expect(textsOf(links)).toEqual([url, url]);
    expectEverySpanSingleRow(links);
    expect(links[0].range).toEqual({
      start: {
        x: 1,
        y: 1
      },
      end: {
        x: 20,
        y: 1
      }
    });
    expect(links[1].range).toEqual({
      start: {
        x: 1,
        y: 2
      },
      end: {
        x: 12,
        y: 2
      }
    });
    // Both per-row links carry the whole URL's spans, so hovering either
    // underlines the entire address.
    expect(links[0].spans).toEqual(links[1].spans);
    expect(links[0].spans).toEqual([
      {
        row: 0,
        startColumn: 0,
        endColumn: 19
      },
      {
        row: 1,
        startColumn: 0,
        endColumn: 11
      }
    ]);
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

    expect(links).toHaveLength(2);
    expect(textsOf(links)).toEqual([url, url]);
    expectEverySpanSingleRow(links);
    expect(links[1].range.end).toEqual({
      x: 12,
      y: 2
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

    expect(links).toHaveLength(2);
    expect(textsOf(links)).toEqual([url, url]);
    expectEverySpanSingleRow(links);
    // Upper span ends at the URL's last glyph (col 19), never the padded edge.
    expect(links[0].range.end).toEqual({
      x: 19,
      y: 1
    });
    // Lower span starts at the indent, not column 0.
    expect(links[1].range.start).toEqual({
      x: 3,
      y: 2
    });
  });

  it("does not make the blank padding of a self-wrapped row clickable", () => {
    // The upper row ends mid-URL far short of a wide terminal's edge; the columns
    // between the URL and the edge are padding an agent left before it wrapped.
    const links = computeLinks({
      terminal: makeTerminal({
        rows: [
          { text: "https://example.com/very" },
          { text: "longpath/here" }
        ],
        columns: 60
      }),
      bufferLineNumber: 1,
      openUrl() {}
    });

    expectEverySpanSingleRow(links);
    // The upper span stops at the last URL glyph (col 24 → x24), so no cell in
    // the 25..60 padding is inside any link range.
    expect(links[0].range.end.x).toBe(24);
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

    expectEverySpanSingleRow(links);
    // The first URL fits on one row; the second wraps, so it contributes two
    // per-row spans — three links, two distinct URLs.
    expect(links).toHaveLength(3);
    expect(links[0].text).toBe(first);
    const secondSpans = links.filter(link => link.text === second);
    expect(secondSpans).toHaveLength(2);
    expect(secondSpans.map(link => link.range.start.y)).toEqual([1, 2]);
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

    // Only the upper row is in this logical line, so only its URL is returned.
    expect(links).toHaveLength(1);
    expect(links[0].text).toBe(first);
    expect(links[0].range.end.y).toBe(1);
  });

  it("rejoins a URL the agent self-wrapped short of a wide terminal's edge", () => {
    // A fullscreen agent wraps at its own content width, not the terminal's: the
    // upper row ends mid-URL well short of the (wide) physical edge, and the
    // continuation is indented under the markdown list item. The edge test can't
    // see this wrap — the URL's own break across the rows must.
    const url = "https://community.getro.com/companies/greylock-2/jobs/74075143-applied-ai-engineer-founding-team";
    const rows = [
      { text: "3. Applied AI Engineer, Founding Team (https://community.getro.com/companies/greylock-2/jobs/74075143-" },
      { text: "   applied-ai-engineer-founding-team)" }
    ];
    for (const bufferLineNumber of [1, 2]) {
      const links = computeLinks({
        terminal: makeTerminal({
          rows,
          columns: 140
        }),
        bufferLineNumber,
        openUrl() {}
      });

      // One span per row, both opening the whole URL; neither crosses rows.
      expect(links).toHaveLength(2);
      expect(textsOf(links)).toEqual([url, url]);
      expectEverySpanSingleRow(links);
      expect(links[0].range.start.y).toBe(1);
      expect(links[1].range.start.y).toBe(2);
      // The lower span ends at the last URL glyph (col 35 → x36); the trailing
      // `)` that closed the markdown link is left out.
      expect(links[1].range.end.x).toBe(36);
    }
  });

  it("stitches a self-wrap whose upper row ends on a domain dot", () => {
    // The upper row ends with `.` — a character the URL pattern trims as trailing
    // punctuation. A continuation test that inspects the ending would give up
    // here; matching across the joined seam must not. Same shape holds for any
    // ending the pattern trims (`,`, `:`, `?`).
    const url = "https://job-boards.greenhouse.io/neosecurityinc/jobs/4323679009";
    const links = computeLinks({
      terminal: makeTerminal({
        rows: [
          { text: "5. Neo (https://job-boards.greenhouse." },
          { text: "   io/neosecurityinc/jobs/4323679009)" }
        ],
        columns: 90
      }),
      bufferLineNumber: 1,
      openUrl() {}
    });

    expect(links).toHaveLength(2);
    expect(textsOf(links)).toEqual([url, url]);
    expect(links[1].range.start.y).toBe(2);
  });

  it("stitches host-only URL shapes (localhost, port, IPv6) across a self-wrap", () => {
    for (const [upper, lower, url] of [
      ["visit http://localhost:300", "0/dashboard here", "http://localhost:3000/dashboard"],
      ["see https://[::1]:8080/api", "/health now", "https://[::1]:8080/api/health"],
      ["at http://192.168.1.10/adm", "in/settings ok", "http://192.168.1.10/admin/settings"]
    ] as const) {
      const links = computeLinks({
        terminal: makeTerminal({
          rows: [
            { text: upper },
            { text: lower }
          ],
          columns: 60
        }),
        bufferLineNumber: 1,
        openUrl() {}
      });

      expect(textsOf(links)).toContain(url);
      expect(links.some(link => link.range.start.y === 2)).toBe(true);
    }
  });

  it("does not glue a fresh marker line onto a URL that ended the row above", () => {
    // The upper row ends exactly at a complete URL; the lower row is a new
    // tool-call line that merely opens with a `%`. A single stray URL byte must
    // not be read as the URL continuing.
    const links = computeLinks({
      terminal: makeTerminal({
        rows: [
          { text: "See https://developer.chrome.com/docs" },
          { text: "% WebFetch https://example.com/other" }
        ],
        columns: 140
      }),
      bufferLineNumber: 1,
      openUrl() {}
    });

    expect(links).toHaveLength(1);
    expect(links[0].text).toBe("https://developer.chrome.com/docs");
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

describe("registerWrappedLinkProvider", () => {
  it("promotes the URL provider above xterm's built-in OSC-8 provider", () => {
    // xterm registers its own OSC-8 provider first; the URL provider must end up
    // ahead of it so its full-URL, wrap-stitched range wins the lowest-index
    // precedence for an OSC-8-tagged URL.
    const oscProvider = { provideLinks() {} };
    const linkProviders: unknown[] = [oscProvider];
    const terminal = {
      ...emptyBuffer,
      registerLinkProvider(provider: unknown) {
        linkProviders.push(provider);
      },
      _core: {
        _linkProviderService: { linkProviders }
      }
    };

    registerWrappedLinkProvider({
      terminal,
      openUrl() {}
    });

    expect(linkProviders).toHaveLength(2);
    expect(linkProviders[0]).not.toBe(oscProvider);
    expect(linkProviders[1]).toBe(oscProvider);
  });

  it("leaves registration untouched when the internals aren't shaped as expected", () => {
    const registered: unknown[] = [];
    const terminal = {
      ...emptyBuffer,
      registerLinkProvider(provider: unknown) {
        registered.push(provider);
      }
    };

    expect(() => registerWrappedLinkProvider({
      terminal,
      openUrl() {}
    })).not.toThrow();
    expect(registered).toHaveLength(1);
  });
});
