// The geometry behind fitting xterm's whole-cell grid into a pane that is not a
// whole number of cells.

export interface GridSize {
  columns: number;
  rows: number;
}

// xterm needs the scrollbar's own width reserved out of the usable columns, or
// the last column hides behind it. Default track width when scrollback is on.
const SCROLLBAR_WIDTH = 14;
const MINIMUM_USABLE_COLUMNS = 20;
const MINIMUM_USABLE_ROWS = 4;

/** The grid a pane of this size implies, in whole cells rounded down so it always
 *  fits inside the pane - never up: the overflowing row would have to be clipped,
 *  and on the normal screen buffer every row is content. Undefined while the pane
 *  (or the font metric) has no usable size: a dock/panel can briefly leave its
 *  flex sibling with no measured space while the browser resolves the new
 *  layout, and clamping that transient to a 2x1 terminal desynchronises a TUI
 *  permanently (Codex treats it as a real resize). The caller keeps the last
 *  truthful grid instead. */
export function gridForPane({ width, height, cell }: {
  width: number;
  height: number;
  cell: {
    width: number;
    height: number;
  } | undefined;
}) {
  const isCellUsable = cell !== undefined && cell.width > 0 && cell.height > 0;
  if (!isCellUsable) {
    return undefined;
  }

  const columns = Math.floor((width - SCROLLBAR_WIDTH) / cell.width);
  const rows = Math.floor(height / cell.height);
  if (columns < MINIMUM_USABLE_COLUMNS || rows < MINIMUM_USABLE_ROWS) {
    return undefined;
  }

  return {
    columns,
    rows
  } satisfies GridSize;
}

/** How much a grid that is momentarily too tall for its pane has to be scaled by
 *  to fit inside it - 1 whenever it fits, which is every settled state. Undefined
 *  while the font metric has no usable height to measure the grid with. */
export function squeezeToFit({ rows, cellHeight, paneHeight }: {
  rows: number;
  cellHeight: number | undefined;
  paneHeight: number;
}) {
  const isCellUsable = cellHeight !== undefined && cellHeight > 0;
  if (!isCellUsable) {
    return undefined;
  }

  const gridHeight = rows * cellHeight;
  return gridHeight > paneHeight && gridHeight > 0 ? paneHeight / gridHeight : 1;
}
