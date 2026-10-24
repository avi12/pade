// One home for a terminal's ANSI/control sequences: stripping them out of a chunk
// of output (so text matchers see the glyphs a TUI wrote rather than the colour
// and cursor-move codes interleaved with them), and - for panes that want to
// *render* the colour instead of hide it - parsing SGR sequences into styled
// segments. Both share the one matcher below, so they always agree on what an
// escape sequence is. Built from strings (the ESC/BEL bytes written as unicode
// escapes) so no raw control character ever sits in the source.

import { ANSI_COLOR_TOKENS } from "@/lib/terminal/theme/terminal-theme";

const ANSI_ESCAPE = new RegExp(
  // OSC: ESC ] … terminated by BEL or ST (ESC \).
  "\\u001b\\][^\\u0007\\u001b]*(?:\\u0007|\\u001b\\\\)"
    // Two-byte escapes: ESC then a single @–Z byte.
    + "|\\u001b[@-Z]"
    // CSI: ESC [ params intermediates final (SGR colours, cursor moves…).
    + "|\\u001b\\[[0-9;?]*[ -/]*[@-~]",
  "g"
);

// A CSI that sets graphics (SGR): ESC [ <numeric params> m. Stricter than the CSI
// branch above - no `?` private-mode marker, no intermediates - so only a real
// colour/weight sequence is interpreted; every other CSI (cursor moves, etc.) is
// consumed and skipped. Built as a string (like the matcher above) so no raw
// control byte sits in the source.
const CSI_SGR = new RegExp("^\\u001b\\[([0-9;]*)m$");

// Cursor Forward (CUF): ESC [ <count> C. Claude's Ink renderer skips over a cell
// that is already blank with this instead of printing a space, so on a fresh
// screen every gap between words arrives as a cursor move.
const CURSOR_FORWARD = new RegExp("\\u001b\\[(\\d*)C", "g");
const CURSOR_FORWARD_DEFAULT_COUNT = 1;
// A ceiling on the gap one cursor move expands to, so a garbage count can't
// allocate an unbounded string - wider than any real terminal row.
const CURSOR_FORWARD_MAXIMUM_COUNT = 1_000;
const DECIMAL_RADIX = 10;
const PARAMETER_SEPARATOR = ";";

function cursorForwardGap(_sequence: string, count: string) {
  const columns = count === "" ? CURSOR_FORWARD_DEFAULT_COUNT : parseInt(count, DECIMAL_RADIX);
  return " ".repeat(Math.min(columns, CURSOR_FORWARD_MAXIMUM_COUNT));
}

/** Strip ANSI/control sequences from a chunk of terminal output, keeping a
 *  cursor-forward skip as the blank gap it leaves on screen - or text matchers
 *  would see `Yes,Itrustthisfolder` where the screen reads "Yes, I trust this
 *  folder". */
export function stripAnsi(text: string) {
  return text.replaceAll(CURSOR_FORWARD, cursorForwardGap).replaceAll(ANSI_ESCAPE, "");
}

/** A run of text sharing one set of SGR styles. Colours resolve to `var(--terminal-*)`
 *  references from the shared palette, so they follow the OS light/dark scheme. */
export interface AnsiSegment {
  text: string;
  color?: string;
  background?: string;
  bold?: boolean;
  dim?: boolean;
  italic?: boolean;
  underline?: boolean;
}

type AnsiStyle = Omit<AnsiSegment, "text">;

// Discrete SGR codes we model. Ranges (standard/bright fg & bg) are handled by
// arithmetic below rather than enumerated here.
const Sgr = {
  Reset: 0,
  Bold: 1,
  Dim: 2,
  Italic: 3,
  Underline: 4,
  NormalIntensity: 22,
  NotItalic: 23,
  NotUnderline: 24,
  ExtendedForeground: 38,
  DefaultForeground: 39,
  ExtendedBackground: 48,
  DefaultBackground: 49
} as const;

// The `38`/`48` extended-colour selector: `5;n` for a palette index, `2;r;g;b`
// for truecolor.
const ExtendedColorMode = {
  Palette: 5,
  TrueColor: 2
} as const;

const STANDARD_FOREGROUND_START = 30;
const STANDARD_BACKGROUND_START = 40;
const BRIGHT_FOREGROUND_START = 90;
const BRIGHT_BACKGROUND_START = 100;
/** Each bank (standard, bright) holds eight colours; bright ones occupy palette
 *  slots 8-15. */
const COLORS_PER_BANK = 8;
const BRIGHT_OFFSET = COLORS_PER_BANK;
/** Parameters a `38`/`48` selector consumes after itself: `5;n`, or `2;r;g;b`. */
const PALETTE_SELECTOR_LENGTH = 2;
const TRUE_COLOR_SELECTOR_LENGTH = 4;

/** Resolve an ANSI colour index (0-15) to the CSS custom-property reference that
 *  paints it - the single palette shared with xterm's theme. */
function ansiColor(iColor: number) {
  return `var(${ANSI_COLOR_TOKENS[iColor]})`;
}

function isInBank({ code, bankStart }: {
  code: number;
  bankStart: number;
}) {
  return code >= bankStart && code < bankStart + COLORS_PER_BANK;
}

/** The palette index a colour code names, or `null` if the code isn't a colour in
 *  this (foreground or background) range. */
function colorIndexFor({ code, standardStart, brightStart }: {
  code: number;
  standardStart: number;
  brightStart: number;
}) {
  if (isInBank({
    code,
    bankStart: standardStart
  })) {
    return code - standardStart;
  }

  if (isInBank({
    code,
    bankStart: brightStart
  })) {
    return code - brightStart + BRIGHT_OFFSET;
  }

  return null;
}

function resetStyle(style: AnsiStyle) {
  style.color = undefined;
  style.background = undefined;
  style.bold = undefined;
  style.dim = undefined;
  style.italic = undefined;
  style.underline = undefined;
}

/** Apply a `38`/`48` extended-colour selector and return the index of the last
 *  parameter it consumed, so the caller resumes after it. Truecolor is consumed
 *  but not modelled; a palette index outside 0–15 is consumed but skipped. */
function applyExtendedColor({ style, codes, iCode, isForeground }: {
  style: AnsiStyle;
  codes: number[];
  iCode: number;
  isForeground: boolean;
}) {
  const mode = codes[iCode + 1];
  if (mode === ExtendedColorMode.TrueColor) {
    return iCode + TRUE_COLOR_SELECTOR_LENGTH;
  }

  if (mode !== ExtendedColorMode.Palette) {
    return iCode;
  }

  const iPaletteColor = codes[iCode + PALETTE_SELECTOR_LENGTH];
  const isInPalette = iPaletteColor !== undefined
    && iPaletteColor >= 0
    && iPaletteColor < ANSI_COLOR_TOKENS.length;
  if (!isInPalette) {
    return iCode + PALETTE_SELECTOR_LENGTH;
  }

  if (isForeground) {
    style.color = ansiColor(iPaletteColor);
  } else {
    style.background = ansiColor(iPaletteColor);
  }

  return iCode + PALETTE_SELECTOR_LENGTH;
}

// The attribute (non-colour) SGR codes, each a mutator on the running style - a
// declarative table so `applySimpleCode` stays a lookup, not a long switch.
const ATTRIBUTE_SETTERS: ReadonlyMap<number, (style: AnsiStyle) => void> = new Map([
  [Sgr.Reset, resetStyle],
  [Sgr.Bold, style => {
    style.bold = true;
  }],
  [Sgr.Dim, style => {
    style.dim = true;
  }],
  [Sgr.Italic, style => {
    style.italic = true;
  }],
  [Sgr.Underline, style => {
    style.underline = true;
  }],
  [Sgr.NormalIntensity, style => {
    style.bold = false; style.dim = false;
  }],
  [Sgr.NotItalic, style => {
    style.italic = false;
  }],
  [Sgr.NotUnderline, style => {
    style.underline = false;
  }],
  [Sgr.DefaultForeground, style => {
    style.color = undefined;
  }],
  [Sgr.DefaultBackground, style => {
    style.background = undefined;
  }]
]);

function applySimpleCode({ style, code }: {
  style: AnsiStyle;
  code: number;
}) {
  const setAttribute = ATTRIBUTE_SETTERS.get(code);
  if (setAttribute) {
    setAttribute(style);
    return;
  }

  const iForeground = colorIndexFor({
    code,
    standardStart: STANDARD_FOREGROUND_START,
    brightStart: BRIGHT_FOREGROUND_START
  });
  if (iForeground !== null) {
    style.color = ansiColor(iForeground);
    return;
  }

  const iBackground = colorIndexFor({
    code,
    standardStart: STANDARD_BACKGROUND_START,
    brightStart: BRIGHT_BACKGROUND_START
  });
  if (iBackground !== null) {
    style.background = ansiColor(iBackground);
  }
  // Any other code is unmodelled - ignore it.
}

/** Mutate `style` per one SGR sequence's parameters (the `32;1` in `ESC[32;1m`).
 *  An empty parameter (or none at all) means reset. */
function applySgr({ style, parameters }: {
  style: AnsiStyle;
  parameters: string;
}) {
  const codes = parameters === ""
    ? [Sgr.Reset]
    : parameters.split(PARAMETER_SEPARATOR).map(part => (part === "" ? Sgr.Reset : parseInt(part, DECIMAL_RADIX)));
  for (let i = 0; i < codes.length; i++) {
    const code = codes[i]!;
    const isExtendedColor = code === Sgr.ExtendedForeground || code === Sgr.ExtendedBackground;
    if (isExtendedColor) {
      i = applyExtendedColor({
        style,
        codes,
        iCode: i,
        isForeground: code === Sgr.ExtendedForeground
      });
      continue;
    }

    applySimpleCode({
      style,
      code
    });
  }
}

function styledSegment({ text, style }: {
  text: string;
  style: AnsiStyle;
}) {
  const segment: AnsiSegment = { text };
  if (style.color !== undefined) {
    segment.color = style.color;
  }

  if (style.background !== undefined) {
    segment.background = style.background;
  }

  if (style.bold) {
    segment.bold = true;
  }

  if (style.dim) {
    segment.dim = true;
  }

  if (style.italic) {
    segment.italic = true;
  }

  if (style.underline) {
    segment.underline = true;
  }

  return segment;
}

/** Parse a chunk of terminal output into ordered styled segments, interpreting SGR
 *  colour/weight sequences and skipping every other control sequence. Pure and
 *  DOM-free; colours are `var(--terminal-*)` references resolved by CSS. Always
 *  returns at least one segment (an empty-text segment for empty/all-escape input),
 *  so a caller can render every line as a row. */
export function parseAnsi(text: string) {
  const segments: AnsiSegment[] = [];
  const style: AnsiStyle = {};
  let runStart = 0;

  for (const match of text.matchAll(ANSI_ESCAPE)) {
    if (match.index > runStart) {
      segments.push(
        styledSegment({
          text: text.slice(runStart, match.index),
          style
        })
      );
    }

    const sgrMatch = CSI_SGR.exec(match[0]);
    if (sgrMatch) {
      applySgr({
        style,
        parameters: sgrMatch[1]!
      });
    }

    runStart = match.index + match[0].length;
  }

  if (runStart < text.length) {
    segments.push(
      styledSegment({
        text: text.slice(runStart),
        style
      })
    );
  }

  if (segments.length === 0) {
    segments.push({ text: "" });
  }

  return segments;
}
