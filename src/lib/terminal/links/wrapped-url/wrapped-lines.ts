// A clickable-URL link provider for xterm that rejoins a URL a program split
// across rows.
//
// xterm's own @xterm/addon-web-links only rejoins rows the TERMINAL soft-wrapped
// - rows whose `isWrapped` flag is set. ADE runs Claude Code with
// CLAUDE_CODE_NO_FLICKER=1, which keeps it on the NORMAL screen (~99 columns
// wide). Claude is an Ink/React TUI: it computes its own layout and self-wraps a
// long URL at its content width, so the continuation row is (a) NOT flagged
// `isWrapped`, and (b) usually INDENTED under Claude's text block - it begins at
// a column well past 0, and the row above may even stop a column shy of the
// physical edge (a right margin). The addon then sees two independent rows and
// detects only the first row's portion, so clicking opens a truncated link.
//
// This provider reconstructs the logical line segment by segment instead: it
// grows the run of rows the clicked row belongs to, joins each row's glyphs from
// its first to its last visible column - stripping every row's leading indent
// and trailing margin so the URL reconnects across the boundary with no spurious
// space - and maps each matched URL back to the exact cells it occupies. A lower
// row continues the upper one when the terminal soft-wrapped it; when the upper
// row reached the right edge and the lower row has content; or when the upper row
// ends mid-URL and the lower row resumes it. That last case matters when the
// terminal is WIDER than the agent's content width: the agent wraps at its own
// narrower width, so the upper row ends mid-URL far short of the physical edge -
// the edge test alone would miss the wrap and truncate the URL.
//
// The registration with xterm lives in wrapped-link-provider.ts.

import { reachesRightEdge, rowContent, rowGlyphs } from "@/lib/terminal/links/wrapped-url/row-content";
import type { CellPosition, LinkBuffer, LinkTerminal } from "@/lib/terminal/links/wrapped-url/row-content";
import { findUrls, hasScheme, isBareToken, URL_PATTERN_GLOBAL } from "@/lib/terminal/links/wrapped-url/url-pattern";
import type { ILink } from "@xterm/xterm";

// Ceiling on how many characters we stitch across, matching the addon's own
// guard so a runaway match can't scan the whole scrollback.
const MAXIMUM_WINDOW_CHARACTERS = 2048;

// Ceiling on how many rows one logical line may span - a second guard on runaway
// expansion when many full rows sit back to back.
const MAXIMUM_LOGICAL_ROWS = 20;

// How many characters past the row boundary the resumed URL must reach before we
// treat the lower row as continuing it, so a fresh line whose first glyph merely
// happens to be a URL byte (Claude's `%` tool-call marker, then a space) isn't
// glued onto the URL above.
const MINIMUM_URL_CONTINUATION = 2;

// Whether a URL begins on the upper row and runs across the boundary into the
// lower row - the tell-tale of a program that self-wrapped a URL at a content
// width narrower than the terminal. It deliberately does NOT inspect how the
// upper row ends: the URL pattern trims a trailing `.`/`,`/`:` (it can't know a
// `.` is a domain dot until the next row's glyphs arrive), so an ending test
// misfires on hosts like `…greenhouse.` while passing `…view/`. Joining the rows
// and letting the URL regex match across the seam is shape-agnostic - it holds
// for host-only, `localhost`, IPv6 `[::1]`, bare IPs, ports, and fragments alike.
// A qualifying match must start on the upper row and reach at least
// `MINIMUM_URL_CONTINUATION` chars past the boundary.
function urlContinuesOntoLower({ upperText, lowerText }: {
  upperText: string;
  lowerText: string;
}) {
  const boundary = upperText.length;
  for (const match of (upperText + lowerText).matchAll(URL_PATTERN_GLOBAL)) {
    const isStartingOnUpperRow = match.index < boundary;
    const isReachingIntoLowerRow = match.index + match[0].length >= boundary + MINIMUM_URL_CONTINUATION;
    if (isStartingOnUpperRow && isReachingIntoLowerRow) {
      return true;
    }
  }

  return false;
}

// The glyphs of rows `topRow..bottomRow` flattened into one string with a parallel
// cell for every character. Each row donates only its first-to-last visible
// column (via `rowGlyphs`), so a leading indent and a trailing margin never leak a
// stray space into the joined URL. The single home for turning a row range into
// text - the growth checks read its text, the caller keeps its cells.
function windowGlyphs({ buffer, columns, topRow, bottomRow }: {
  buffer: LinkBuffer;
  columns: number;
  topRow: number;
  bottomRow: number;
}) {
  let text = "";
  const cells: CellPosition[] = [];
  for (let row = topRow; row <= bottomRow && text.length < MAXIMUM_WINDOW_CHARACTERS; row += 1) {
    const line = buffer.getLine(row);
    if (!line) {
      continue;
    }

    const content = rowContent({
      line,
      columns
    });
    if (!content) {
      continue;
    }

    const glyphs = rowGlyphs({
      line,
      content
    });
    for (let i = 0; i < glyphs.text.length && text.length < MAXIMUM_WINDOW_CHARACTERS; i += 1) {
      text += glyphs.text[i];
      cells.push({
        row,
        column: glyphs.columns[i]
      });
    }
  }

  return {
    text,
    cells
  };
}

// Whether the row below the window continues the same logical line. The terminal's
// own soft wrap always continues; a hard wrap continues when the window's last row
// ran to the right edge; and a self-wrap continues when a URL from the window (its
// scheme is on the top row) runs across the boundary into the lower row. Testing
// the WHOLE window, not just its last row, is what lets a URL that already spans
// several rows keep growing - its later rows carry no scheme of their own.
function continuesBelow({ buffer, columns, topRow, bottomRow }: {
  buffer: LinkBuffer;
  columns: number;
  topRow: number;
  bottomRow: number;
}) {
  const lower = buffer.getLine(bottomRow + 1);
  if (!lower) {
    return false;
  }

  const lowerContent = rowContent({
    line: lower,
    columns
  });
  if (!lowerContent) {
    return false;
  }

  if (lower.isWrapped) {
    return true;
  }

  const lastLine = buffer.getLine(bottomRow);
  const lastContent = lastLine && rowContent({
    line: lastLine,
    columns
  });
  if (lastContent && reachesRightEdge({
    content: lastContent,
    columns
  })) {
    return true;
  }

  // The self-wrap branch is the ambiguous one: geometrically, a URL that ends
  // short of the edge followed by an unrelated line (git's `remote:` status, a
  // `f8506e9..01dc34e  main` commit range) looks identical to an agent that wrapped
  // a URL at its own narrower content width. What tells them apart is the lower
  // row's shape: an agent indents the continuation under its text block, and the
  // continuation is one bare token (a URL has no spaces). A line that starts at
  // column 0, or carries a space, is a fresh line - not the tail of this URL - so
  // it is never glued on. (Soft and hard wraps are already handled above by their
  // geometry, so a legitimately column-0 continuation still stitches through them.)
  const lowerText = rowGlyphs({
    line: lower,
    content: lowerContent
  }).text;
  const isIndentedContinuation = lowerContent.firstColumn > 0 && isBareToken(lowerText);
  if (!isIndentedContinuation) {
    return false;
  }

  return urlContinuesOntoLower({
    upperText: windowGlyphs({
      buffer,
      columns,
      topRow,
      bottomRow
    }).text,
    lowerText
  });
}

// Whether the row above the window continues the same logical line - the mirror of
// `continuesBelow`, plus one move it can't make: while the window is still a
// scheme-less URL fragment (its start not yet reached), absorb the bare token or
// the scheme row directly above, so a URL hovered on its LAST row still walks up
// through its scheme-less middle rows to the row that starts it. The scheme guard
// keeps this from gluing an unrelated line above a URL that's already whole.
function continuesAbove({ buffer, columns, topRow, bottomRow }: {
  buffer: LinkBuffer;
  columns: number;
  topRow: number;
  bottomRow: number;
}) {
  const upper = buffer.getLine(topRow - 1);
  if (!upper) {
    return false;
  }

  const upperContent = rowContent({
    line: upper,
    columns
  });
  if (!upperContent) {
    return false;
  }

  if (buffer.getLine(topRow)?.isWrapped) {
    return true;
  }

  if (reachesRightEdge({
    content: upperContent,
    columns
  })) {
    return true;
  }

  const upperText = rowGlyphs({
    line: upper,
    content: upperContent
  }).text;
  const windowText = windowGlyphs({
    buffer,
    columns,
    topRow,
    bottomRow
  }).text;
  if (urlContinuesOntoLower({
    upperText,
    lowerText: windowText
  })) {
    return true;
  }

  return !hasScheme(windowText) && (hasScheme(upperText) || isBareToken(upperText));
}

// Grow the run of rows the clicked row belongs to - up to the URL's start, then
// down to its end - and flatten it into one string with a parallel cell for every
// character. Growing up first lets the downward walk see the scheme once a URL
// hovered mid-way has been rejoined to its first row.
function buildLogicalLine({ buffer, columns, anchorRow }: {
  buffer: LinkBuffer;
  columns: number;
  anchorRow: number;
}) {
  let topRow = anchorRow;
  let bottomRow = anchorRow;

  while (
    topRow > 0
    && bottomRow - topRow + 1 < MAXIMUM_LOGICAL_ROWS
    && continuesAbove({
      buffer,
      columns,
      topRow,
      bottomRow
    })
  ) {
    topRow -= 1;
  }

  while (
    bottomRow - topRow + 1 < MAXIMUM_LOGICAL_ROWS
    && continuesBelow({
      buffer,
      columns,
      topRow,
      bottomRow
    })
  ) {
    bottomRow += 1;
  }

  return windowGlyphs({
    buffer,
    columns,
    topRow,
    bottomRow
  });
}

// One contiguous run of a URL's glyphs on a single buffer row: the tight column
// span the URL occupies there.
export interface RowSpan {
  row: number;
  startColumn: number;
  endColumn: number;
}

// Group a URL's per-character cells into one span per row. Cells arrive in
// reading order - left to right within a row, top to bottom across rows - so a
// same-row cell only ever extends the current span's end.
function rowSpans(urlCells: CellPosition[]) {
  const spans: RowSpan[] = [];
  for (const cell of urlCells) {
    const current = spans.at(-1);
    if (current && current.row === cell.row) {
      current.endColumn = cell.column;
      continue;
    }

    spans.push({
      row: cell.row,
      startColumn: cell.column,
      endColumn: cell.column
    });
  }

  return spans;
}

export interface WrappedLink extends ILink {
  spans: RowSpan[];
}

// Detect every clickable URL passing through the clicked buffer line. A URL that
// wraps across rows becomes ONE link PER ROW: a single multi-row `range` would
// make xterm treat every cell between start and end as the link, so the blank
// padding a self-wrapping agent leaves before the wrap would be clickable (and,
// for the built-in underline, filled to the edge). Per-row single-row ranges hug
// the glyphs instead. Each per-row link carries the URL's full span list and its
// whole text, so hovering any row underlines the entire URL and a click anywhere
// opens it whole. (xterm's own web-links addon uses one multi-row range, but only
// stitches genuine soft-wraps, where the upper row is full to the edge with no
// padding to expose.)
export function computeLinks({ terminal, bufferLineNumber, openUrl }: {
  terminal: LinkTerminal;
  bufferLineNumber: number;
  openUrl: (uri: string) => void;
}) {
  const { text, cells } = buildLogicalLine({
    buffer: terminal.buffer.active,
    columns: terminal.cols,
    anchorRow: bufferLineNumber - 1
  });
  const links: WrappedLink[] = [];
  for (const { url: uri, start } of findUrls(text)) {
    const spans = rowSpans(cells.slice(start, start + uri.length));
    for (const span of spans) {
      links.push({
        text: uri,
        // Ranges are 1-based and end-inclusive; source columns are 0-based, so
        // every edge gains 1.
        range: {
          start: {
            x: span.startColumn + 1,
            y: span.row + 1
          },
          end: {
            x: span.endColumn + 1,
            y: span.row + 1
          }
        },
        // Built-in underline off - the provider paints its own over every span
        // (see `withHoverUnderline`); keep the pointer cursor so it reads as a link.
        decorations: {
          underline: false,
          pointerCursor: true
        },
        spans,
        activate: (_event, activatedUri) => openUrl(activatedUri)
      });
    }
  }

  return links;
}
