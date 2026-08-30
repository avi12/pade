// Painting PADE's terminal with a Windows Terminal colour scheme.
//
// Applying one means writing the `--terminal-*` design tokens onto the document
// root — never handing colours to xterm directly. Those tokens are already what
// xterm's theme (`lib/terminal-theme`) and the runner dock's SGR parser both
// read, so one install repaints every surface that speaks ANSI and none of them
// can drift onto a second palette. Clearing them hands the terminal straight
// back to the app theme's own tokens (theme.css), which is the default.
//
// Pure: `lib/prefs` owns the DOM write, this owns the mapping.

import { ANSI_COLOR_TOKENS } from "@/lib/terminal-theme";
import { Scheme, type TerminalScheme } from "@/lib/types";

/** The terminal's surface tokens, alongside the 16 ANSI ones. The cursor has a
 *  token of its own so a scheme can name it; theme.css falls it back to the
 *  app's primary. */
const CODE_BACKGROUND = "--code-background";
const CODE_FOREGROUND = "--code-foreground";
const TERMINAL_CURSOR = "--terminal-cursor";
const TERMINAL_SELECTION = "--terminal-selection";

/** Every property an installed scheme writes — and therefore exactly what has
 *  to be removed again to hand the terminal back to the app theme. */
export const TERMINAL_PALETTE_PROPERTIES = [
  CODE_BACKGROUND,
  CODE_FOREGROUND,
  TERMINAL_CURSOR,
  TERMINAL_SELECTION,
  ...ANSI_COLOR_TOKENS
] as const;

/** How opaque a selection wash is when the scheme names no selection colour of
 *  its own — Windows Terminal composites its own there, so PADE derives one
 *  from the foreground rather than leaving xterm to invent a white wash. */
const DERIVED_SELECTION_ALPHA = 0.3;

const HEX_COLOR = /^#([0-9a-f]{3}|[0-9a-f]{6})$/i;
const SHORT_HEX_DIGITS = 3;
const RGB_CHANNELS = 3;
const HEX_RADIX = 16;
const CHANNEL_MAX = 255;
/** The sRGB → linear light knee, and the coefficients of the luminance the eye
 *  actually perceives (Rec. 709, as WCAG uses them). */
const SRGB_KNEE = 0.03928;
const SRGB_KNEE_DIVISOR = 12.92;
const SRGB_OFFSET = 0.055;
const SRGB_SCALE = 1.055;
const SRGB_EXPONENT = 2.4;
const LUMINANCE_WEIGHTS = [0.2126, 0.7152, 0.0722] as const;
/** Below this perceived luminance a background reads as a dark terminal. Well
 *  clear of both ends in practice: Solarized Light lands near 0.9, Solarized
 *  Dark near 0.02. */
const DARK_BACKGROUND_LUMINANCE = 0.4;

/** `#rgb` / `#rrggbb` → its three 0..255 channels; `null` for anything else, so
 *  a colour PADE can't read never becomes a silently wrong one. */
function hexChannels(color: string): [number, number, number] | null {
  const match = HEX_COLOR.exec(color.trim());
  if (!match) {
    return null;
  }

  const digits = match[1];
  const perChannel = digits.length === SHORT_HEX_DIGITS ? 1 : 2;
  const channels: number[] = [];
  for (let channel = 0; channel < RGB_CHANNELS; channel++) {
    const raw = digits.slice(channel * perChannel, (channel + 1) * perChannel);
    channels.push(parseInt(perChannel === 1 ? raw + raw : raw, HEX_RADIX));
  }

  return [channels[0], channels[1], channels[2]];
}

/** The same colour with an alpha channel appended, as the `#rrggbbaa` xterm's
 *  own parser accepts (see `lib/terminal-theme` on why parse-safety matters). */
function withAlpha(color: string, alpha: number): string {
  const channels = hexChannels(color);
  if (!channels) {
    return color;
  }

  const opacity = Math.round(alpha * CHANNEL_MAX)
    .toString(HEX_RADIX)
    .padStart(2, "0");
  const hex = channels.map(channel => channel.toString(HEX_RADIX).padStart(2, "0")).join("");
  return `#${hex}${opacity}`;
}

/** Perceived luminance, 0 (black) to 1 (white). */
function luminance(channels: [number, number, number]): number {
  let total = 0;
  for (let channel = 0; channel < channels.length; channel++) {
    const value = channels[channel] / CHANNEL_MAX;
    const linear =
      value <= SRGB_KNEE
        ? value / SRGB_KNEE_DIVISOR
        : ((value + SRGB_OFFSET) / SRGB_SCALE) ** SRGB_EXPONENT;
    total += linear * LUMINANCE_WEIGHTS[channel];
  }

  return total;
}

/** Light or dark, as the terminal's own background answers it. This is what the
 *  agent is told (`theming.rs` themes each CLI from it): a Claude painting its
 *  dark theme onto Solarized Light is unreadable, and the app's own light/dark
 *  says nothing about a terminal the user gave a palette of its own. An
 *  unreadable colour falls back to `fallback` — the app's scheme. */
export function schemeOfBackground({
  color,
  fallback
}: {
  color: string;
  fallback: Scheme;
}): Scheme {
  const channels = hexChannels(color);
  if (!channels) {
    return fallback;
  }

  return luminance(channels) < DARK_BACKGROUND_LUMINANCE ? Scheme.enum.dark : Scheme.enum.light;
}

/** The custom properties this scheme installs on the document root. */
export function terminalPaletteProperties(scheme: TerminalScheme): Map<string, string> {
  const properties = new Map<string, string>([
    [CODE_BACKGROUND, scheme.background],
    [CODE_FOREGROUND, scheme.foreground],
    [TERMINAL_CURSOR, scheme.cursor ?? scheme.foreground],
    [TERMINAL_SELECTION, scheme.selection ?? withAlpha(scheme.foreground, DERIVED_SELECTION_ALPHA)]
  ]);
  for (let index = 0; index < ANSI_COLOR_TOKENS.length; index++) {
    properties.set(ANSI_COLOR_TOKENS[index], scheme.ansi[index]);
  }

  return properties;
}
