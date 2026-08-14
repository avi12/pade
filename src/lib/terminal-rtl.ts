// Right-to-left text in a terminal — the one place that decides WHICH COLUMNS of
// a row read right to left, and what to draw over them.
//
// A terminal grid is a left-to-right array of cells: the agent picks a column and
// paints one glyph there. Arabic, Hebrew, Syriac and friends are neither. Their
// letters *join* — each takes a different form depending on its neighbours — and
// they read right to left. So a cell-by-cell renderer paints them disconnected and
// backwards: `مرحبا` comes out as `ا ب ح ر م`.
//
// Nothing here reshapes or reorders a single character. The browser already
// implements the Unicode Bidirectional Algorithm and OpenType shaping, and it is
// authoritative — far more so than anything we would hand-roll. This module's whole
// job is to say which columns to hand it as one string, so the buffer, the copied
// text, the link provider and every output parser keep reading the agent's own
// logical order, untouched.
//
// The unit is a RUN, not a line. A line overlay would take the box borders and
// aligned columns of an agent's TUI with it and let them drift off the cell grid;
// a run is anchored to exactly the cells it covers, so everything around it stays
// pixel-identical to what xterm painted.

import { ansiPaletteColor } from "@/lib/terminal-theme";
import type { IBufferCell, IBufferLine, IBufferRange, Terminal } from "@xterm/xterm";

/** Scripts that read right to left. Presentation forms (`ﻡ`, `ﷺ`) carry the same
 *  Script property as the letters they are forms of, so they match too. */
const RTL_SCRIPT =
  /[\p{Script=Arabic}\p{Script=Hebrew}\p{Script=Syriac}\p{Script=Thaana}\p{Script=Nko}\p{Script=Adlam}]/u;

/** Any letter — only ever consulted after {@link RTL_SCRIPT} has said no, so a
 *  match here means "a letter that reads left to right", which ends a run. */
const LTR_LETTER = /\p{L}/u;

// Box drawing through geometric shapes, plus the Powerline private-use block: the
// glyphs a TUI draws its FRAME with. They are symbols, so the bidi algorithm calls
// them neutral and would happily carry them inside a right-to-left run and move
// them — turning an agent's box border inside out. They are layout, not text, so
// they end a run instead. (xterm draws the same line for its own contrast rules:
// `treatGlyphAsBackgroundColor`.)
const BOX_DRAWING_FIRST = 0x2500;
const GEOMETRIC_SHAPES_LAST = 0x25ff;
const POWERLINE_FIRST = 0xe0b0;
const POWERLINE_LAST = 0xe0bf;

/** What one cell contributes to the direction of the text around it. */
const CellDirection = {
  Rtl: "rtl",
  Ltr: "ltr",
  /** Spaces, punctuation, digits — they take the direction of their neighbours. */
  Neutral: "neutral",
  /** A frame glyph: layout, and a hard end to any run. */
  Frame: "frame"
} as const;
type CellDirection = (typeof CellDirection)[keyof typeof CellDirection];

/** How many neutral cells may sit *inside* one run. A single space between words
 *  is prose and must stay inside the run, or every word would be reordered on its
 *  own and the sentence would read backwards. A wide gap is column padding — an
 *  agent aligning a table — and joining across it would let one cell's text slide
 *  into the next column's space. */
const RUN_NEUTRAL_GAP_LIMIT = 8;

function isFrameGlyph(text: string): boolean {
  const codePoint = text.codePointAt(0) ?? 0;
  const isBoxOrShape = codePoint >= BOX_DRAWING_FIRST && codePoint <= GEOMETRIC_SHAPES_LAST;
  return isBoxOrShape || (codePoint >= POWERLINE_FIRST && codePoint <= POWERLINE_LAST);
}

function cellDirection(text: string): CellDirection {
  // The trailing half of a wide glyph holds no characters of its own.
  if (text === "") {
    return CellDirection.Neutral;
  }

  if (isFrameGlyph(text)) {
    return CellDirection.Frame;
  }

  if (RTL_SCRIPT.test(text)) {
    return CellDirection.Rtl;
  }

  if (LTR_LETTER.test(text)) {
    return CellDirection.Ltr;
  }

  return CellDirection.Neutral;
}

/** Does this text contain anything that reads right to left? The cheap latch that
 *  decides whether a session pays for the overlay at all. */
export function containsRtl(text: string): boolean {
  return RTL_SCRIPT.test(text);
}

/** A half-open span of columns: `startColumn` up to but not including `endColumn`. */
export interface ColumnRange {
  readonly startColumn: number;
  readonly endColumn: number;
}

/** The columns of one row that read right to left, in column order. A run always
 *  begins and ends on a right-to-left character — the neutrals around it belong to
 *  the text on either side, and covering them would hide cells needlessly. */
export function rtlRuns({ cells }: { cells: readonly string[] }): ColumnRange[] {
  const runs: ColumnRange[] = [];
  let firstStrong = -1;
  let lastStrong = -1;
  let neutralsSinceStrong = 0;

  function closeRun() {
    if (firstStrong >= 0) {
      runs.push({
        startColumn: firstStrong,
        endColumn: lastStrong + 1
      });
    }

    firstStrong = -1;
    lastStrong = -1;
    neutralsSinceStrong = 0;
  }

  for (let column = 0; column < cells.length; column++) {
    const direction = cellDirection(cells[column] ?? "");
    if (direction === CellDirection.Rtl) {
      if (firstStrong < 0) {
        firstStrong = column;
      }

      lastStrong = column;
      neutralsSinceStrong = 0;
      continue;
    }

    if (direction === CellDirection.Neutral) {
      neutralsSinceStrong += 1;

      if (neutralsSinceStrong > RUN_NEUTRAL_GAP_LIMIT) {
        closeRun();
      }

      continue;
    }

    closeRun();
  }

  closeRun();
  return runs;
}

/** Everything about a cell that decides how it is painted, colours already
 *  resolved to CSS by the caller (which owns the palette). */
export interface RtlStyle {
  readonly foreground: string;
  readonly background: string;
  readonly bold: boolean;
  readonly dim: boolean;
  readonly italic: boolean;
  readonly underline: boolean;
  readonly strikethrough: boolean;
}

/** One cell of a row: its characters (`""` for the trailing half of a wide glyph,
 *  a base plus its combining marks for an Arabic letter carrying harakat) and how
 *  it is painted. */
export interface RtlCell extends RtlStyle {
  readonly text: string;
}

/** What a run is made of, in visual sequence. */
export const RtlPartKind = {
  Text: "text",
  /** The text cursor. It is a MARKER IN THE SEQUENCE, not a measured position:
   *  a logical column means nothing once a line has been reordered, so the caret
   *  is placed between parts and the browser lays it out with the text — it
   *  cannot end up disagreeing with the glyphs. */
  Caret: "caret"
} as const;
type RtlPartKind = (typeof RtlPartKind)[keyof typeof RtlPartKind];

/** A stretch of a run painted identically — the unit the browser shapes.
 *  Splitting is kept to the minimum the styling forces, because letters do not
 *  join across an element boundary: every extra part is a broken join. */
export interface RtlTextPart extends RtlStyle {
  readonly kind: typeof RtlPartKind.Text;
  readonly text: string;
  readonly selected: boolean;
}

export interface RtlCaretPart {
  readonly kind: typeof RtlPartKind.Caret;
}

export type RtlPart = RtlTextPart | RtlCaretPart;

/** One right-to-left run, ready to draw over the cells it covers. */
export interface RtlRun {
  readonly startColumn: number;
  readonly columns: number;
  /** The run's own backdrop — its first cell's background, so the space the shaped
   *  text does not fill still matches what xterm painted underneath. */
  readonly background: string;
  readonly parts: readonly RtlPart[];
}

const CARET: RtlCaretPart = { kind: RtlPartKind.Caret };

function sameStyle({ part, cell }: {
  part: RtlStyle;
  cell: RtlStyle;
}): boolean {
  return (
    part.foreground === cell.foreground
    && part.background === cell.background
    && part.bold === cell.bold
    && part.dim === cell.dim
    && part.italic === cell.italic
    && part.underline === cell.underline
    && part.strikethrough === cell.strikethrough
  );
}

function isColumnSelected({ column, selection }: {
  column: number;
  selection?: ColumnRange;
}): boolean {
  if (!selection) {
    return false;
  }

  return column >= selection.startColumn && column < selection.endColumn;
}

/** The part being built with this cell added, or nothing when the cell has to
 *  start a new one — a change of painting, of selection, or the caret landing
 *  here. */
function extendedPart({ open, cell, selected, startsCaret }: {
  open: RtlTextPart | undefined;
  cell: RtlCell;
  selected: boolean;
  startsCaret: boolean;
}): RtlTextPart | undefined {
  if (!open || startsCaret || open.selected !== selected) {
    return undefined;
  }

  if (!sameStyle({
    part: open,
    cell
  })) {
    return undefined;
  }

  return {
    ...open,
    text: open.text + cell.text
  };
}

/** The parts of one run: its text split only where the painting, the selection or
 *  the caret forces it, with the caret marker dropped in where it belongs. */
function runParts({ cells, range, cursorColumn, selection }: {
  cells: readonly RtlCell[];
  range: ColumnRange;
  cursorColumn?: number;
  selection?: ColumnRange;
}): RtlPart[] {
  const parts: RtlPart[] = [];
  let open: RtlTextPart | undefined;

  for (let column = range.startColumn; column < range.endColumn; column++) {
    const cell = cells[column];
    // A missing cell, or the trailing half of a wide glyph: nothing to paint.
    if (!cell || cell.text === "") {
      continue;
    }

    const selected = isColumnSelected({
      column,
      selection
    });
    const startsCaret = column === cursorColumn;
    const extended = extendedPart({
      open,
      cell,
      selected,
      startsCaret
    });
    if (extended) {
      open = extended;
      continue;
    }

    if (open) {
      parts.push(open);
    }

    if (startsCaret) {
      parts.push(CARET);
    }

    open = {
      ...cell,
      kind: RtlPartKind.Text,
      selected
    };
  }

  if (open) {
    parts.push(open);
  }

  // Past the run's last character — where a composer leaves it while you type.
  if (cursorColumn === range.endColumn) {
    parts.push(CARET);
  }

  return parts;
}

/** Build the runs to draw over one row of cells.
 *
 *  `cursorColumn` is the cursor's column when it is on this row, `selection` the
 *  columns of this row that are selected. */
export function rtlRow({ cells, cursorColumn, selection }: {
  cells: readonly RtlCell[];
  cursorColumn?: number;
  selection?: ColumnRange;
}): RtlRun[] {
  const runs: RtlRun[] = [];

  for (const range of rtlRuns({ cells: cells.map(cell => cell.text) })) {
    runs.push({
      startColumn: range.startColumn,
      columns: range.endColumn - range.startColumn,
      background: cells[range.startColumn]?.background ?? "",
      parts: runParts({
        cells,
        range,
        cursorColumn,
        selection
      })
    });
  }

  return runs;
}

// ── Reading it out of xterm's buffer ──────────────────────────────────────────
// The adapter, kept here beside the segmentation it feeds and away from the DOM,
// so the overlay component is left with nothing to do but draw. Type-only import:
// nothing of xterm is pulled into the bundle by it.

/** One run, placed on the row it was found on. `key` keeps Svelte's keyed
 *  `{#each}` stable while the viewport scrolls under it. */
export interface PlacedRtlRun extends RtlRun {
  readonly row: number;
  readonly key: string;
}

const DEFAULT_FOREGROUND = "var(--code-foreground)";
const DEFAULT_BACKGROUND = "var(--code-background)";
/** Truecolor arrives as one 24-bit number. */
const TRUECOLOR_HEX_LENGTH = 6;

function cellColor({ palette, truecolor, value, fallback }: {
  palette: boolean;
  truecolor: boolean;
  value: number;
  fallback: string;
}): string {
  if (palette) {
    return ansiPaletteColor({ index: value });
  }

  if (truecolor) {
    return `#${value.toString(16).padStart(TRUECOLOR_HEX_LENGTH, "0")}`;
  }

  return fallback;
}

/** One xterm cell as the overlay needs it, its colours resolved through the same
 *  design tokens as xterm's own palette — so the overlay follows a light/dark
 *  flip with the terminal under it and never has to be rebuilt for one. */
function rtlCell({ cell }: { cell: IBufferCell }): RtlCell {
  const foreground = cellColor({
    palette: cell.isFgPalette(),
    truecolor: cell.isFgRGB(),
    value: cell.getFgColor(),
    fallback: DEFAULT_FOREGROUND
  });
  const background = cellColor({
    palette: cell.isBgPalette(),
    truecolor: cell.isBgRGB(),
    value: cell.getBgColor(),
    fallback: DEFAULT_BACKGROUND
  });
  const inverse = cell.isInverse() !== 0;

  return {
    // xterm paints an invisible cell blank; the run still needs its width.
    text: cell.isInvisible() !== 0 ? " " : cell.getChars(),
    foreground: inverse ? background : foreground,
    background: inverse ? foreground : background,
    bold: cell.isBold() !== 0,
    dim: cell.isDim() !== 0,
    italic: cell.isItalic() !== 0,
    underline: cell.isUnderline() !== 0,
    strikethrough: cell.isStrikethrough() !== 0
  };
}

/** One row's cells, read through the single scratch cell xterm hands back from
 *  every `getCell` — reading a row must not allocate an object per column. */
function rowCells({ line, scratch }: {
  line: IBufferLine;
  scratch: IBufferCell;
}): RtlCell[] {
  const cells: RtlCell[] = [];
  for (let column = 0; column < line.length; column++) {
    cells.push(rtlCell({ cell: line.getCell(column, scratch) ?? scratch }));
  }

  return cells;
}

/** The columns of one row the selection covers. xterm reports only the two ends,
 *  in absolute buffer coordinates, so a row in the middle of a multi-row
 *  selection is selected end to end. */
export function rowSelection({ selection, absoluteRow, columns }: {
  selection: IBufferRange | undefined;
  absoluteRow: number;
  columns: number;
}): ColumnRange | undefined {
  const outsideSelection =
    !selection || absoluteRow < selection.start.y || absoluteRow > selection.end.y;
  if (outsideSelection || !selection) {
    return undefined;
  }

  return {
    startColumn: absoluteRow === selection.start.y ? selection.start.x : 0,
    endColumn: absoluteRow === selection.end.y ? selection.end.x : columns
  };
}

/** Every right-to-left run on screen. Rows without one cost a string and a regex;
 *  the per-cell reads happen only on the rows that actually need drawing. */
export function rtlViewport({ terminal, focused }: {
  terminal: Terminal;
  focused: boolean;
}): PlacedRtlRun[] {
  const buffer = terminal.buffer.active;
  const selection = terminal.getSelectionPosition();
  const scratch = buffer.getNullCell();
  const placed: PlacedRtlRun[] = [];

  for (let row = 0; row < terminal.rows; row++) {
    const absoluteRow = buffer.viewportY + row;
    const line = buffer.getLine(absoluteRow);
    if (!line || !containsRtl(line.translateToString(true))) {
      continue;
    }

    const rowRuns = rtlRow({
      cells: rowCells({
        line,
        scratch
      }),
      cursorColumn: focused && row === buffer.cursorY ? buffer.cursorX : undefined,
      selection: rowSelection({
        selection,
        absoluteRow,
        columns: line.length
      })
    });

    for (const run of rowRuns) {
      placed.push({
        ...run,
        row,
        key: `${row}:${run.startColumn}`
      });
    }
  }

  return placed;
}
