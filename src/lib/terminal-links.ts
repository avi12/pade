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
// row continues the upper one when the terminal soft-wrapped it; when the upper
// row reached the right edge and the lower row has content; or when the upper row
// ends mid-URL and the lower row resumes it. That last case matters when the
// terminal is WIDER than the agent's content width: the agent wraps at its own
// narrower width, so the upper row ends mid-URL far short of the physical edge —
// the edge test alone would miss the wrap and truncate the URL.
//
// The URL pattern and the URL-validity check are ported from
// @xterm/addon-web-links (MIT, the xterm.js authors) because its internals
// aren't exported. Keep them in sync if that addon is upgraded.
import type { ILink, ILinkProvider } from "@xterm/xterm";

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

// The Unicode Box Drawing (U+2500–U+257F) and Block Elements (U+2580–U+259F)
// blocks. These are frame glyphs, never URL content: a `─` rule under a link, or
// the `▌`/`│` bar Claude's TUI draws down the left edge of a pasted block. A bar
// on a continuation row would otherwise be joined into the logical line and cut
// the URL match at the row boundary, so frame glyphs at a row's edges are
// excluded from its content span (a row of nothing but frame glyphs — a
// separator rule — thereby reads as blank and is never stitched).
const FRAME_GLYPH_FIRST = 0x2500;
const FRAME_GLYPH_LAST = 0x259f;

function isFrameGlyph(characters: string): boolean {
  const codePoint = characters.codePointAt(0);
  return codePoint !== undefined && codePoint >= FRAME_GLYPH_FIRST && codePoint <= FRAME_GLYPH_LAST;
}

interface RowContent {
  /** First column holding a visible non-frame glyph. */
  firstColumn: number;
  /** Last column holding a visible non-frame glyph. */
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
  // The cell's SGR underline attribute — non-zero when the AGENT itself rendered
  // the run as underlined (opencode paints its links this way). xterm returns a
  // number (0, or an underline-style code), so callers test truthiness, never
  // `=== true`. Optional so a test mock can omit it; absent reads as not
  // underlined.
  isUnderline?(): number | boolean;
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

// The first and last columns of a row that hold a visible non-frame glyph, or
// null when the row is blank or all frame (a separator rule). Leading indent —
// blanks AND a frame bar — and the trailing margin fall outside this span;
// `rawLastColumn` still records where the row truly ends, frame included.
function rowContent({ line, columns }: {
  line: LinkLine;
  columns: number;
}): RowContent | null {
  let firstColumn = -1;
  let lastColumn = -1;
  for (let column = 0; column < columns; column += 1) {
    const characters = line.getCell(column)?.getChars();
    const isBlank = characters === undefined || characters === BLANK_CELL || characters === EMPTY_CELL;
    if (isBlank || isFrameGlyph(characters)) {
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

// Whether a row's TEXT ran to (or within a column of) the right edge, the
// tell-tale of a program that wrapped because it ran out of width. Frame glyphs
// don't count: a row whose text stops mid-row and is merely padded to the edge
// with a `─` rule (Claude's tool-call underline) did not wrap — stitching the
// row below it would glue an unrelated line onto the URL.
function reachesRightEdge({ content, columns }: {
  content: RowContent;
  columns: number;
}): boolean {
  return content.lastColumn >= columns - 1 - RIGHT_EDGE_SLACK;
}

// One row's glyphs from its first to its last visible column, plus the source
// column each character came from. The single home for turning a row's content
// span into text — both the logical-line builder and the URL-continuation test
// read from it, so they can never disagree on what a row's text is.
function rowGlyphs({ line, content }: {
  line: LinkLine;
  content: RowContent;
}): { text: string; columns: number[] } {
  let text = "";
  const columns: number[] = [];
  for (let column = content.firstColumn; column <= content.lastColumn; column += 1) {
    const characters = line.getCell(column)?.getChars();
    const isTrailingWideHalf = characters === undefined || characters === EMPTY_CELL;
    if (isTrailingWideHalf) {
      continue;
    }

    text += characters;
    for (let offset = 0; offset < characters.length; offset += 1) {
      columns.push(column);
    }
  }

  return {
    text,
    columns
  };
}

// The URL that runs to the very end of `text`, or null when the text doesn't end
// mid-URL. A row ending inside a URL is the tell-tale of a wrap: the program had
// more URL to write but ran out of its content width.
function trailingUrl(text: string): string | null {
  const pattern = new RegExp(URL_PATTERN.source, "g");
  let last: RegExpExecArray | null = null;
  for (let match = pattern.exec(text); match !== null; match = pattern.exec(text)) {
    last = match;
  }

  if (!last || last.index + last[0].length !== text.length) {
    return null;
  }

  return last[0];
}

// How many extra URL characters the lower row must contribute before we treat it
// as continuing the upper row's URL, so a fresh line whose leading glyph merely
// happens to be a URL byte (Claude's `%` tool-call marker, then a space) doesn't
// get glued onto the URL above it.
const MIN_URL_CONTINUATION = 2;

// Whether the lower row resumes a URL the upper row broke mid-way. Unlike the
// geometric edge test, this catches a fullscreen agent that self-wrapped a URL
// at a content width NARROWER than the terminal, leaving the upper row ending
// mid-URL well short of the physical right edge (an indented continuation under a
// wide pane). The upper row must end inside a URL, and appending the lower row's
// text must extend that same URL by more than a stray byte.
function urlContinuesOntoLower({ upperText, lowerText }: {
  upperText: string;
  lowerText: string;
}): boolean {
  const trailing = trailingUrl(upperText);
  if (!trailing) {
    return false;
  }

  const continued = new RegExp(URL_PATTERN.source).exec(trailing + lowerText);
  if (!continued || continued.index !== 0) {
    return false;
  }

  return continued[0].length >= trailing.length + MIN_URL_CONTINUATION;
}

// Whether the row just below `row` continues it as one logical line. The
// terminal's own soft wrap always continues; a hard wrap continues when `row` ran
// its text to the right edge and the row below has content; and a self-wrap
// continues when `row` ends mid-URL and the row below resumes it — exactly how a
// program wraps a URL it couldn't fit on one row, at the terminal edge or at its
// own narrower content width.
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

  if (reachesRightEdge({
    content: upperContent,
    columns
  })) {
    return true;
  }

  return urlContinuesOntoLower({
    upperText: rowGlyphs({
      line: upper,
      content: upperContent
    }).text,
    lowerText: rowGlyphs({
      line: lower,
      content: lowerContent
    }).text
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
      const characters = line.getCell(column)?.getChars();
      const isTrailingWideHalf = characters === undefined || characters === EMPTY_CELL;
      if (isTrailingWideHalf) {
        continue;
      }

      text += characters;
      for (let offset = 0; offset < characters.length; offset += 1) {
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

// Whether the agent already painted this URL's cells with the SGR underline
// attribute (opencode renders its links as underlined blue text, parens and
// all). When it did, xterm's own link-hover underline is redundant — and worse,
// it fills the wrapped row out to the right edge, underlining the blank padding
// the agent left before the wrap. Sampling the URL's first cell is enough: the
// whole run shares one style.
function agentAlreadyUnderlines({ buffer, startCell }: {
  buffer: LinkBuffer;
  startCell: CellPosition;
}): boolean {
  const cell = buffer.getLine(startCell.row)?.getCell(startCell.column);
  return Boolean(cell?.isUnderline?.());
}

// Detect every clickable URL that passes through the clicked buffer line, mapped
// back to its exact start and end cells so the hover range covers the whole URL
// even when it self-wrapped across indented rows.
export function computeLinks({ terminal, bufferLineNumber, openUrl }: {
  terminal: LinkTerminal;
  bufferLineNumber: number;
  openUrl: (uri: string) => void;
}): ILink[] {
  const buffer = terminal.buffer.active;
  const { text, cells } = buildLogicalLine({
    buffer,
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
      const link: ILink = {
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
      };
      // Don't add a second underline (and its wrapped-row edge padding) over a
      // URL the agent already underlines — keep it clickable, styled as the
      // agent drew it.
      if (agentAlreadyUnderlines({
        buffer,
        startCell
      })) {
        link.decorations = {
          underline: false
        };
      }

      links.push(link);
    }

    match = pattern.exec(text);
  }

  return links;
}

// The slice of xterm's `Terminal` this registration needs: everything
// `computeLinks` reads (via `LinkTerminal`) plus `registerLinkProvider`. Narrow
// enough that a test builds a plain mock, while the real `Terminal` satisfies it.
interface RegisterableTerminal extends LinkTerminal {
  registerLinkProvider(provider: ILinkProvider): unknown;
}

// xterm's ordered list of link providers — its own OSC-8 provider is registered
// first at terminal construction, and this list is where its precedence lives.
interface InternalLinkProviderService {
  linkProviders: ILinkProvider[];
}

// Read the internal link-provider service off the terminal without a type
// assertion: `_core` and the service are private, so each hop is fetched
// reflectively and runtime-checked. A renamed field just yields null, and the
// caller leaves the provider where it is (still correct, only lower-priority).
function linkProviderService(terminal: RegisterableTerminal): InternalLinkProviderService | null {
  const core: unknown = Reflect.get(terminal, "_core");
  if (core === null || typeof core !== "object") {
    return null;
  }

  const service: unknown =
    Reflect.get(core, "_linkProviderService") ?? Reflect.get(core, "linkProviderService");
  if (service === null || typeof service !== "object") {
    return null;
  }

  const linkProviders: unknown = Reflect.get(service, "linkProviders");
  if (!Array.isArray(linkProviders)) {
    return null;
  }

  return { linkProviders };
}

// Move `provider` to the front of xterm's provider list. xterm picks the
// LOWEST-index provider that has a link under the cursor, and it registers its
// own OSC-8 provider first — so an agent that emits a URL as an OSC-8 hyperlink
// (Claude, OpenCode) would otherwise be served by that provider, which
// underlines only the agent-tagged cells (a single self-wrapped row, or the
// wrapping parens) instead of the whole URL. Promoting this provider makes its
// regex-parsed, wrap-stitched, punctuation-trimmed range win for URL-shaped
// links; a labelled OSC-8 link (whose visible text isn't a URL) matches nothing
// here and still falls through to the OSC-8 provider behind it.
function promoteAboveOscLinks({ terminal, provider }: {
  terminal: RegisterableTerminal;
  provider: ILinkProvider;
}): void {
  const service = linkProviderService(terminal);
  if (!service) {
    return;
  }

  const index = service.linkProviders.indexOf(provider);
  if (index <= 0) {
    return;
  }

  service.linkProviders.splice(index, 1);
  service.linkProviders.unshift(provider);
}

// Register a link provider that makes http(s) URLs clickable, rejoining soft-,
// hard-, and self-wrapped rows so a wrapped URL activates and underlines in full.
// `openUrl` receives the whole URL — route it to the system browser. It is
// promoted above xterm's built-in OSC-8 provider so an agent's OSC-8-tagged URL
// gets the same full-URL, no-trailing-punctuation range as a plain-text one;
// labelled OSC-8 links still reach that provider.
export function registerWrappedLinkProvider({ terminal, openUrl }: {
  terminal: RegisterableTerminal;
  openUrl: (uri: string) => void;
}): void {
  const provider: ILinkProvider = {
    provideLinks(bufferLineNumber, callback) {
      callback(
        computeLinks({
          terminal,
          bufferLineNumber,
          openUrl
        })
      );
    }
  };
  terminal.registerLinkProvider(provider);
  promoteAboveOscLinks({
    terminal,
    provider
  });
}
