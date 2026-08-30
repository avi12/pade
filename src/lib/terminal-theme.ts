// The xterm theme is the agent's source of truth for light vs dark — so it must
// always be truthful and never silently dropped.
//
// PADE's decision on agent/app theme sync: PADE follows the OS scheme end-to-end
// (prefs seed from the native window theme, then follow it; theme.css carries
// full light and dark palettes; Terminal.svelte re-themes xterm when the scheme
// flips), and the agent follows the TERMINAL, not the OS.
//
// What that buys an already-running agent on Windows is: nothing. Claude Code's
// `auto` theme resolves as `live override ?? $COLORFGBG ?? dark`, the override is
// only set by an OSC 11 background-color answer, and that query never reaches the
// terminal here — so the scheme is fixed by the environment the session was
// spawned with. Terminal.svelte still relays the standard
// `CSI ?997;<dark|light>n` report (harmless, and the documented channel), but
// measurement says it does not move a live Claude — see `theming.rs` and
// `docs/terminal-rendering.md`. Keeping these colors truthful matters for what
// the user reads, not for what the agent believes.
//
// Truthful requires parse-safe. xterm's color parser takes `#hex` (alpha
// included) and legacy comma `rgb()`/`rgba()` directly; every other format falls
// back to a canvas probe that REJECTS non-opaque colors. theme.css authors its
// tokens in modern `hsl()` — the opaque ones survive the canvas path, but an
// alpha token like `--terminal-selection: hsl(210deg 90% 45% / 22%)` threw there
// and was silently replaced by xterm's default (a white wash, invisible on the
// light scheme). So every token is converted to hex before xterm sees it.

import type { ITheme } from "@xterm/xterm";

/** The 16 ANSI palette slots as their `--terminal-*` design-token names, in ANSI
 *  order (0–7 standard, 8–15 bright). The one home mapping an ANSI color index to
 *  the token that paints it — xterm's theme ({@link xtermTheme}) and the runner
 *  dock's SGR parser (`parseAnsi`) both read it, so the terminal and the runner
 *  output can never drift onto two different palettes. */
export const ANSI_COLOR_TOKENS = [
  "--terminal-black",
  "--terminal-red",
  "--terminal-green",
  "--terminal-yellow",
  "--terminal-blue",
  "--terminal-magenta",
  "--terminal-cyan",
  "--terminal-white",
  "--terminal-bright-black",
  "--terminal-bright-red",
  "--terminal-bright-green",
  "--terminal-bright-yellow",
  "--terminal-bright-blue",
  "--terminal-bright-magenta",
  "--terminal-bright-cyan",
  "--terminal-bright-white"
] as const;

// The 240 slots above the 16 named ones are not a palette anyone chose — they are
// a formula every terminal computes the same way: a 6×6×6 colour cube, then a
// 24-step grey ramp. Spelling them out as a table would be 240 magic numbers.
const COLOR_CUBE_FIRST = 16;
const COLOR_CUBE_SIDE = 6;
/** The six levels a cube channel can take — deliberately not linear; this is the
 *  ramp xterm and every other terminal use. */
const COLOR_CUBE_LEVELS = [0, 95, 135, 175, 215, 255] as const;
const GREY_RAMP_FIRST = 232;
const GREY_RAMP_BASE = 8;
const GREY_RAMP_STEP = 10;

/** An ANSI colour index as a CSS colour. The first 16 resolve through the design
 *  tokens ({@link ANSI_COLOR_TOKENS}) so they follow the app's light/dark scheme
 *  exactly as xterm's own palette does; the rest are the standard 256-colour
 *  formula, which is scheme-independent by definition. */
export function ansiPaletteColor({ index }: { index: number }): string {
  const token = ANSI_COLOR_TOKENS[index];
  if (token) {
    return `var(${token})`;
  }

  if (index >= GREY_RAMP_FIRST) {
    const level = GREY_RAMP_BASE + (index - GREY_RAMP_FIRST) * GREY_RAMP_STEP;
    return `rgb(${level} ${level} ${level})`;
  }

  const offset = index - COLOR_CUBE_FIRST;
  const red = COLOR_CUBE_LEVELS[Math.floor(offset / (COLOR_CUBE_SIDE * COLOR_CUBE_SIDE)) % COLOR_CUBE_SIDE];
  const green = COLOR_CUBE_LEVELS[Math.floor(offset / COLOR_CUBE_SIDE) % COLOR_CUBE_SIDE];
  const blue = COLOR_CUBE_LEVELS[offset % COLOR_CUBE_SIDE];
  return `rgb(${red} ${green} ${blue})`;
}

/** xterm's 16 ANSI theme slots, in the same order as {@link ANSI_COLOR_TOKENS}. */
const ANSI_THEME_KEYS = [
  "black",
  "red",
  "green",
  "yellow",
  "blue",
  "magenta",
  "cyan",
  "white",
  "brightBlack",
  "brightRed",
  "brightGreen",
  "brightYellow",
  "brightBlue",
  "brightMagenta",
  "brightCyan",
  "brightWhite"
] as const satisfies readonly (keyof ITheme)[];

const HSL_COLOR = new RegExp(
  "^hsl\\(\\s*(-?\\d+(?:\\.\\d+)?)(?:deg)?" + // hue, `deg` optional
    "\\s+(\\d+(?:\\.\\d+)?)%" + // saturation
    "\\s+(\\d+(?:\\.\\d+)?)%" + // lightness
    "(?:\\s*/\\s*(\\d*(?:\\.\\d+)?)(%?))?" + // optional `/ alpha` (number or %)
    "\\s*\\)$",
  "i"
);

function channelHex(value: number): string {
  const clamped = Math.min(255, Math.max(0, Math.round(value * 255)));
  return clamped.toString(16).padStart(2, "0");
}

/** Convert a modern `hsl()` token to `#rrggbb[aa]`; any other format (already-hex,
 *  `rgb()`, a future `color-mix()`) passes through for xterm's own parser. */
export function xtermSafeColor(raw: string): string {
  const match = HSL_COLOR.exec(raw.trim());
  if (!match) {
    return raw;
  }

  const [, hueRaw, saturationRaw, lightnessRaw, alphaRaw, alphaPercent] = match;
  const hue = ((parseFloat(hueRaw) % 360) + 360) % 360;
  const saturation = Math.min(100, parseFloat(saturationRaw)) / 100;
  const lightness = Math.min(100, parseFloat(lightnessRaw)) / 100;

  const chroma = (1 - Math.abs(2 * lightness - 1)) * saturation;
  const secondary = chroma * (1 - Math.abs(((hue / 60) % 2) - 1));
  const base = lightness - chroma / 2;

  const sextant = Math.floor(hue / 60) % 6;
  const [red, green, blue] = [
    [chroma, secondary, 0],
    [secondary, chroma, 0],
    [0, chroma, secondary],
    [0, secondary, chroma],
    [secondary, 0, chroma],
    [chroma, 0, secondary]
  ][sextant]!;

  let hex = `#${channelHex(red + base)}${channelHex(green + base)}${channelHex(blue + base)}`;
  const hasAlpha = alphaRaw !== undefined && alphaRaw !== "";
  if (hasAlpha) {
    const alpha = alphaPercent === "%" ? parseFloat(alphaRaw) / 100 : parseFloat(alphaRaw);
    if (alpha < 1) {
      hex += channelHex(alpha);
    }
  }

  return hex;
}

/** Build xterm's theme from the design tokens (the `--terminal-*` ANSI palette +
 *  code surface), every color made parse-safe. `readToken` supplies the computed
 *  value of one custom property — the caller owns the DOM read, this mapping
 *  stays pure. Agent CLIs paint with these 16 slots, and xterm's own defaults
 *  only suit a dark screen — the light scheme re-picks every one dark enough to
 *  read. */
export function xtermTheme({ readToken }: { readToken: (name: string) => string }): ITheme {
  function color(name: string): string {
    return xtermSafeColor(readToken(name));
  }

  const ansiColors: Partial<ITheme> = {};
  for (let index = 0; index < ANSI_THEME_KEYS.length; index++) {
    ansiColors[ANSI_THEME_KEYS[index]] = color(ANSI_COLOR_TOKENS[index]);
  }

  return {
    background: color("--code-background"),
    foreground: color("--code-foreground"),
    cursor: color("--terminal-cursor"),
    selectionBackground: color("--terminal-selection"),
    ...ansiColors
  };
}
