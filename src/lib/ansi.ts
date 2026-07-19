// One home for a terminal's ANSI/control sequences: stripping them out of a chunk
// of output (so text matchers see the glyphs a TUI wrote rather than the colour
// and cursor-move codes interleaved with them), and — for panes that want to
// *render* the colour instead of hide it — parsing SGR sequences into styled
// segments. Both share the one matcher below, so they always agree on what an
// escape sequence is. Built from strings (the ESC/BEL bytes written as unicode
// escapes) so no raw control character ever sits in the source.

import { ANSI_COLOR_TOKENS } from "@/lib/terminal-theme";

const ANSI_ESCAPE_RE = new RegExp(
  // OSC: ESC ] … terminated by BEL or ST (ESC \).
  "\\u001b\\][^\\u0007\\u001b]*(?:\\u0007|\\u001b\\\\)"
    // Two-byte escapes: ESC then a single @–Z byte.
    + "|\\u001b[@-Z]"
    // CSI: ESC [ params intermediates final (SGR colours, cursor moves…).
    + "|\\u001b\\[[0-9;?]*[ -/]*[@-~]",
  "g"
);

// A CSI that sets graphics (SGR): ESC [ <numeric params> m. Stricter than the CSI
// branch above — no `?` private-mode marker, no intermediates — so only a real
// colour/weight sequence is interpreted; every other CSI (cursor moves, etc.) is
// consumed and skipped. Built as a string (like the matcher above) so no raw
// control byte sits in the source.
const CSI_SGR_RE = new RegExp("^\\u001b\\[([0-9;]*)m$");

/** Strip ANSI/control sequences from a chunk of terminal output. */
export function stripAnsi(text: string): string {
  return text.replaceAll(ANSI_ESCAPE_RE, "");
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
// Bright colours occupy palette slots 8–15.
const BRIGHT_OFFSET = 8;

/** Resolve an ANSI colour index (0–15) to the CSS custom-property reference that
 *  paints it — the single palette shared with xterm's theme. */
function ansiColor(index: number): string {
  return `var(${ANSI_COLOR_TOKENS[index]})`;
}

/** The palette index a colour code names, or `null` if the code isn't a colour in
 *  this (foreground or background) range. */
function colorIndexFor({ code, standardStart, brightStart }: {
  code: number;
  standardStart: number;
  brightStart: number;
}): number | null {
  if (code >= standardStart && code <= standardStart + 7) {
    return code - standardStart;
  }

  if (code >= brightStart && code <= brightStart + 7) {
    return code - brightStart + BRIGHT_OFFSET;
  }

  return null;
}

function resetStyle(style: AnsiStyle): void {
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
function applyExtendedColor({ style, codes, index, foreground }: {
  style: AnsiStyle;
  codes: number[];
  index: number;
  foreground: boolean;
}): number {
  const mode = codes[index + 1];
  if (mode === ExtendedColorMode.Palette) {
    const paletteIndex = codes[index + 2];
    const inPalette = paletteIndex !== undefined
      && paletteIndex >= 0
      && paletteIndex < ANSI_COLOR_TOKENS.length;
    if (inPalette) {
      if (foreground) {
        style.color = ansiColor(paletteIndex);
      } else {
        style.background = ansiColor(paletteIndex);
      }
    }

    return index + 2;
  }

  if (mode === ExtendedColorMode.TrueColor) {
    return index + 4;
  }

  return index;
}

// The attribute (non-colour) SGR codes, each a mutator on the running style — a
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
}): void {
  const setAttribute = ATTRIBUTE_SETTERS.get(code);
  if (setAttribute) {
    setAttribute(style);
    return;
  }

  const foreground = colorIndexFor({
    code,
    standardStart: STANDARD_FOREGROUND_START,
    brightStart: BRIGHT_FOREGROUND_START
  });
  if (foreground !== null) {
    style.color = ansiColor(foreground);
    return;
  }

  const background = colorIndexFor({
    code,
    standardStart: STANDARD_BACKGROUND_START,
    brightStart: BRIGHT_BACKGROUND_START
  });
  if (background !== null) {
    style.background = ansiColor(background);
  }
  // Any other code is unmodelled — ignore it.
}

/** Mutate `style` per one SGR sequence's parameters (the `32;1` in `ESC[32;1m`). */
function applySgr({ style, params }: {
  style: AnsiStyle;
  params: string;
}): void {
  const parts = params === "" ? ["0"] : params.split(";");
  const codes = parts.map(part => (part === "" ? 0 : parseInt(part, 10)));
  for (let index = 0; index < codes.length; index++) {
    const code = codes[index]!;
    if (code === Sgr.ExtendedForeground || code === Sgr.ExtendedBackground) {
      index = applyExtendedColor({
        style,
        codes,
        index,
        foreground: code === Sgr.ExtendedForeground
      });
      continue;
    }

    applySimpleCode({
      style,
      code
    });
  }
}

function styledSegment(text: string, style: AnsiStyle): AnsiSegment {
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
export function parseAnsi(text: string): AnsiSegment[] {
  const segments: AnsiSegment[] = [];
  const style: AnsiStyle = {};
  let runStart = 0;

  ANSI_ESCAPE_RE.lastIndex = 0;
  for (let match = ANSI_ESCAPE_RE.exec(text); match !== null; match = ANSI_ESCAPE_RE.exec(text)) {
    if (match.index > runStart) {
      segments.push(styledSegment(text.slice(runStart, match.index), style));
    }

    const sgr = CSI_SGR_RE.exec(match[0]);
    if (sgr) {
      applySgr({
        style,
        params: sgr[1]!
      });
    }

    runStart = ANSI_ESCAPE_RE.lastIndex;
  }

  if (runStart < text.length) {
    segments.push(styledSegment(text.slice(runStart), style));
  }

  if (segments.length === 0) {
    segments.push({ text: "" });
  }

  return segments;
}
