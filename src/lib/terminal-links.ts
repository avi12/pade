// A clickable-URL link provider for xterm that rejoins a URL a program split
// across rows.
//
// xterm's own @xterm/addon-web-links only rejoins rows the TERMINAL soft-wrapped
// — rows whose `isWrapped` flag is set. ADE runs Claude Code with
// CLAUDE_CODE_NO_FLICKER=1, which keeps it on the NORMAL screen (~99 columns
// wide). Claude is an Ink/React TUI: it computes its own layout and self-wraps a
// long URL at its content width, so the continuation row is (a) NOT flagged
// `isWrapped`, and (b) usually INDENTED under Claude's text block — it begins at
// a column well past 0, and the row above may even stop a column shy of the
// physical edge (a right margin). The addon then sees two independent rows and
// detects only the first row's portion, so clicking opens a truncated link.
//
// This provider reconstructs the logical line segment by segment instead: it
// grows the run of rows the clicked row belongs to, joins each row's glyphs from
// its first to its last visible column — stripping every row's leading indent
// and trailing margin so the URL reconnects across the boundary with no spurious
// space — and maps each matched URL back to the exact cells it occupies. A lower
// row continues the upper one when the terminal soft-wrapped it, or when the
// upper row reached the right edge and the lower row has content: precisely the
// self-wrap this fixes.
//
// The URL pattern and the URL-validity check are ported from
// @xterm/addon-web-links (MIT, the xterm.js authors) because its internals
// aren't exported. Keep them in sync if that addon is upgraded.
import type { ILink, Terminal } from "@xterm/xterm";

// Matches an http(s) URL. Copied verbatim from @xterm/addon-web-links so both
// paths detect exactly the same links. Its trailing character class excludes
// sentence punctuation, so a URL ending a sentence doesn't swallow the period.
const URL_PATTERN = /(https?|HTTPS?):[/]{2}[^\s"'!*(){}|\\^<>`]*[^\s"':,.!?{}|\\^~[\]`()<>]/;

// Ceiling on how many characters we stitch across, matching the addon's own
// guard so a runaway match can't scan the whole scrollback.
const MAX_WINDOW_CHARS = 2048;

// Ceiling on how many rows one logical line may span — a second guard on runaway
// expansion when many full rows sit back to back.
const MAX_LOGICAL_ROWS = 20;

// How many columns short of the last physical column still counts as reaching
// the edge: Claude often wraps a column shy of the true margin.
const RIGHT_EDGE_SLACK = 1;

// A single space or an unwritten cell — the two ways a cell renders blank —
// either of which is skipped when measuring a row's glyph span.
const BLANK_CELL = " ";
const EMPTY_CELL = "";

interface RowContent {
  firstColumn: number;
  lastColumn: number;
}

interface CellPosition {
  row: number;
  column: number;
}

interface LogicalLine {
  text: string;
  cells: CellPosition[];
}

// The slice of xterm's `Terminal` that link computation actually reads. Narrow
// enough that a test builds a plain mock with no cast, while the real `Terminal`
// satisfies it structurally.
interface LinkCell {
  getChars(): string;
}
interface LinkLine {
  isWrapped: boolean;
  getCell(column: number): LinkCell | undefined;
}
interface LinkBuffer {
  getLine(index: number): LinkLine | undefined;
}
interface LinkTerminal {
  cols: number;
  buffer: { active: LinkBuffer };
}

// The `protocol//[user[:pass]@]host` prefix a real URL must start with — built
// with guard clauses so the credential variants read top-to-bottom.
function authorityOf(url: URL): string {
  const base = `${url.protocol}//`;
  if (url.username && url.password) {
    return `${base}${url.username}:${url.password}@${url.host}`;
  }

  if (url.username) {
    return `${base}${url.username}@${url.host}`;
  }

  return `${base}${url.host}`;
}

// The matched text really forms a URL (protocol//[user[:pass]@]host…). Ported
// from the addon: it rejects near-misses the permissive pattern would otherwise
// accept (a bare `https://` with no host).
function isUrl(candidate: string): boolean {
  try {
    const authority = authorityOf(new URL(candidate));
    return candidate.toLocaleLowerCase().startsWith(authority.toLocaleLowerCase());
  } catch {
    return false;
  }
}

// The first and last columns of a row that hold a visible glyph, or null when
// the row is blank. Leading indent and trailing margin fall outside this span.
function rowContent({ line, columns }: {
  line: LinkLine;
  columns: number;
}): RowContent | null {
  let firstColumn = -1;
  let lastColumn = -1;
  for (let column = 0; column < columns; column += 1) {
    const chars = line.getCell(column)?.getChars();
    const isBlank = chars === undefined || chars === BLANK_CELL || chars === EMPTY_CELL;
    if (isBlank) {
      continue;
    }

    if (firstColumn === -1) {
      firstColumn = column;
    }

    lastColumn = column;
  }

  if (firstColumn === -1) {
    return null;
  }

  return {
    firstColumn,
    lastColumn
  };
}

// Whether a row's content ran to (or within a column of) the right edge, the
// tell-tale of a program that wrapped because it ran out of width.
function reachesRightEdge({ content, columns }: {
  content: RowContent;
  columns: number;
}): boolean {
  return content.lastColumn >= columns - 1 - RIGHT_EDGE_SLACK;
}

// Whether the row just below `row` continues it as one logical line. The
// terminal's own soft wrap always continues; otherwise a hard wrap continues
// only when `row` ran its text to the right edge and the row below has content —
// exactly how a program wraps a URL it couldn't fit on one row.
function nextRowContinues({ buffer, columns, row }: {
  buffer: LinkBuffer;
  columns: number;
  row: number;
}): boolean {
  const upper = buffer.getLine(row);
  const lower = buffer.getLine(row + 1);
  if (!upper || !lower) {
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

  const upperContent = rowContent({
    line: upper,
    columns
  });
  if (!upperContent) {
    return false;
  }

  return reachesRightEdge({
    content: upperContent,
    columns
  });
}

// Grow the run of rows the clicked row belongs to, then flatten it into one
// string with a parallel cell position for every character. Each row donates
// only its glyphs from first to last visible column, so a leading indent and a
// trailing margin never leak a stray space into the joined URL.
function buildLogicalLine({ buffer, columns, anchorRow }: {
  buffer: LinkBuffer;
  columns: number;
  anchorRow: number;
}): LogicalLine {
  let topRow = anchorRow;
  let bottomRow = anchorRow;

  while (
    topRow > 0
    && bottomRow - topRow + 1 < MAX_LOGICAL_ROWS
    && nextRowContinues({
      buffer,
      columns,
      row: topRow - 1
    })
  ) {
    topRow -= 1;
  }

  while (
    bottomRow - topRow + 1 < MAX_LOGICAL_ROWS
    && nextRowContinues({
      buffer,
      columns,
      row: bottomRow
    })
  ) {
    bottomRow += 1;
  }

  let text = "";
  const cells: CellPosition[] = [];
  for (let row = topRow; row <= bottomRow && text.length < MAX_WINDOW_CHARS; row += 1) {
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

    for (
      let column = content.firstColumn;
      column <= content.lastColumn && text.length < MAX_WINDOW_CHARS;
      column += 1
    ) {
      const chars = line.getCell(column)?.getChars();
      const isTrailingWideHalf = chars === undefined || chars === EMPTY_CELL;
      if (isTrailingWideHalf) {
        continue;
      }

      text += chars;
      for (let offset = 0; offset < chars.length; offset += 1) {
        cells.push({
          row,
          column
        });
      }
    }
  }

  return {
    text,
    cells
  };
}

// Detect every clickable URL that passes through the clicked buffer line, mapped
// back to its exact start and end cells so the hover range covers the whole URL
// even when it self-wrapped across indented rows.
export function computeLinks({ terminal, bufferLineNumber, openUrl }: {
  terminal: LinkTerminal;
  bufferLineNumber: number;
  openUrl: (uri: string) => void;
}): ILink[] {
  const { text, cells } = buildLogicalLine({
    buffer: terminal.buffer.active,
    columns: terminal.cols,
    anchorRow: bufferLineNumber - 1
  });
  // A fresh global copy per call so `lastIndex` never leaks between lines.
  const pattern = new RegExp(URL_PATTERN.source, "g");

  const links: ILink[] = [];
  let match = pattern.exec(text);
  while (match) {
    const uri = match[0];
    if (isUrl(uri)) {
      const startCell = cells[match.index];
      const endCell = cells[match.index + uri.length - 1];
      links.push({
        text: uri,
        // Ranges are 1-based and end-inclusive; both cells carry 0-based buffer
        // positions, so every edge gains 1.
        range: {
          start: {
            x: startCell.column + 1,
            y: startCell.row + 1
          },
          end: {
            x: endCell.column + 1,
            y: endCell.row + 1
          }
        },
        activate: (_event: MouseEvent, activatedUri: string) => openUrl(activatedUri)
      });
    }

    match = pattern.exec(text);
  }

  return links;
}

// Register a link provider that makes plain-text http(s) URLs clickable,
// rejoining both soft- and self-wrapped rows so a wrapped URL activates in full.
// `openUrl` receives the whole URL — route it to the system browser. OSC-8
// hyperlinks are handled separately by the terminal's own `linkHandler`.
export function registerWrappedLinkProvider({ terminal, openUrl }: {
  terminal: Terminal;
  openUrl: (uri: string) => void;
}): void {
  terminal.registerLinkProvider({
    provideLinks(bufferLineNumber, callback) {
      callback(
        computeLinks({
          terminal,
          bufferLineNumber,
          openUrl
        })
      );
    }
  });
}
