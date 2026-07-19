// The xterm theme is the agent's source of truth for light vs dark — so it must
// always be truthful and never silently dropped.
//
// PADE's decision on agent/app theme sync: PADE follows the OS scheme end-to-end
// (prefs resolve themeMode "system" via matchMedia; theme.css carries full light
// and dark palettes; Terminal.svelte re-themes xterm when the scheme flips), and
// the agent follows the TERMINAL, not the OS. Claude Code's os-sync theme asks
// the terminal: an OSC 11 query for the background color (it picks light/dark by
// the reply's luminance, with COLORFGBG as fallback and dark as default), plus
// DECSET 2031 to be told again whenever the palette changes. xterm answers the
// query from the theme built here and pushes a `CSI ?997;n` report on every
// re-theme — so keeping this theme truthful is the whole sync mechanism; no env
// var or extra plumbing is needed.
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
    cursor: color("--primary"),
    selectionBackground: color("--terminal-selection"),
    ...ansiColors
  };
}
