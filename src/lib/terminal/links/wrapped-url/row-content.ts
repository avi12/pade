// A buffer row read as glyphs: where its visible content starts and ends, what each
// cell shows, and whether the row reaches the terminal's right edge.

// How many columns short of the last physical column still counts as reaching
// the edge: Claude often wraps a column shy of the true margin.
const RIGHT_EDGE_SLACK = 1;

// A single space or an unwritten cell - the two ways a cell renders blank -
// either of which is skipped when measuring a row's glyph span.
const BLANK_CELL = " ";

const EMPTY_CELL = "";

/** "No visible column found yet" while scanning a row. */
const NO_COLUMN = -1;

// The Unicode Box Drawing (U+2500–U+257F) and Block Elements (U+2580–U+259F)
// blocks. These are frame glyphs, never URL content: a `─` rule under a link, or
// the `▌`/`│` bar Claude's TUI draws down the left edge of a pasted block. A bar
// on a continuation row would otherwise be joined into the logical line and cut
// the URL match at the row boundary, so frame glyphs at a row's edges are
// excluded from its content span (a row of nothing but frame glyphs - a
// separator rule - thereby reads as blank and is never stitched).
const FRAME_GLYPH_FIRST = 0x2500;

const FRAME_GLYPH_LAST = 0x259f;

function isFrameGlyph(characters: string) {
  const codePoint = characters.codePointAt(0);
  return codePoint !== undefined && codePoint >= FRAME_GLYPH_FIRST && codePoint <= FRAME_GLYPH_LAST;
}

interface RowContent {
  /** First column holding a visible non-frame glyph. */
  firstColumn: number;
  /** Last column holding a visible non-frame glyph. */
  lastColumn: number;
}

export interface CellPosition {
  row: number;
  column: number;
}

// The slice of xterm's `Terminal` that link computation actually reads. Narrow
// enough that a test builds a plain mock with no cast, while the real `Terminal`
// satisfies it structurally.
interface LinkCell {
  getChars(): string;
  getWidth(): number;
}

interface LinkLine {
  isWrapped: boolean;
  getCell(column: number): LinkCell | undefined;
}

export interface LinkBuffer {
  getLine(index: number): LinkLine | undefined;
}

export interface LinkTerminal {
  cols: number;
  buffer: { active: LinkBuffer };
}

// The first and last columns of a row that hold a visible non-frame glyph, or
// null when the row is blank or all frame (a separator rule). Leading indent -
// blanks AND a frame bar - and the trailing margin fall outside this span;
// `rawLastColumn` still records where the row truly ends, frame included.
export function rowContent({ line, columns }: { line: LinkLine; columns: number }) {
  let firstColumn = NO_COLUMN;
  let lastColumn = NO_COLUMN;
  for (let column = 0; column < columns; column += 1) {
    const characters = line.getCell(column)?.getChars();
    const isBlank = characters === undefined || characters === BLANK_CELL || characters === EMPTY_CELL;
    const isSkippableCell = isBlank || isFrameGlyph(characters);
    if (isSkippableCell) {
      continue;
    }

    if (firstColumn === NO_COLUMN) {
      firstColumn = column;
    }

    lastColumn = column;
  }

  if (firstColumn === NO_COLUMN) {
    return null;
  }

  return {
    firstColumn,
    lastColumn
  };
}

// Whether a row's TEXT ran to (or within a column of) the right edge, the
// tell-tale of a program that wrapped because it ran out of width. Frame glyphs
// don't count: a row whose text stops mid-row and is merely padded to the edge
// with a `─` rule (Claude's tool-call underline) did not wrap - stitching the
// row below it would glue an unrelated line onto the URL.
export function reachesRightEdge({ content, columns }: { content: RowContent; columns: number }) {
  return content.lastColumn >= columns - 1 - RIGHT_EDGE_SLACK;
}

// The trailing half of a wide glyph is the one cell xterm gives zero width.
const WIDE_GLYPH_TRAILING_HALF_WIDTH = 0;

// The text a cell contributes to a row: nothing for the trailing half of a wide
// glyph (its leading half already carries the character), a space for an
// unwritten cell, and its glyphs otherwise. An unwritten cell mid-row is a gap
// the program skipped with a cursor move rather than a printed space - Claude's
// Ink renderer emits `ESC[1C` between words - so it must still read as a word
// break, or the URL regex runs straight through `….` into the next word.
export function cellText(cell: LinkCell | undefined) {
  const isEmptyCell = !cell || cell.getWidth() === WIDE_GLYPH_TRAILING_HALF_WIDTH;
  if (isEmptyCell) {
    return EMPTY_CELL;
  }

  const characters = cell.getChars();
  return characters === EMPTY_CELL ? BLANK_CELL : characters;
}

// One row's glyphs from its first to its last visible column, plus the source
// column each character came from. The single home for turning a row's content
// span into text - both the logical-line builder and the URL-continuation test
// read from it, so they can never disagree on what a row's text is.
export function rowGlyphs({ line, content }: { line: LinkLine; content: RowContent }) {
  let text = "";
  const columns: number[] = [];
  for (let column = content.firstColumn; column <= content.lastColumn; column += 1) {
    const characters = cellText(line.getCell(column));
    text += characters;
    columns.push(...Array.from({ length: characters.length }, () => column));
  }

  return {
    text,
    columns
  };
}
