// Painting PADE's terminal with a Windows Terminal colour scheme.
//
// Applying one means writing the `--terminal-*` design tokens onto the document
// root - never handing colours to xterm directly. Those tokens are already what
// xterm's theme (`lib/terminal/terminal-theme`) and the runner dock's SGR parser both
// read, so one install repaints every surface that speaks ANSI and none of them
// can drift onto a second palette. Clearing them hands the terminal straight
// back to the app theme's own tokens (theme.css), which is the default.
//
// Pure: `lib/settings/prefs` owns the DOM write, this owns the mapping.

import { hexChannels, hexLuminance } from "@/lib/code/colors";
import {
  ANSI_COLOR_TOKENS,
  CODE_BACKGROUND_TOKEN,
  CODE_FOREGROUND_TOKEN,
  TERMINAL_CURSOR_TOKEN,
  TERMINAL_SELECTION_TOKEN
} from "@/lib/terminal/terminal-theme";
import { Scheme, type TerminalScheme } from "@/lib/types";

/** The syntax roles, each named by the terminal token whose colour it takes -
 *  the ANSI slot a palette already spends on that role. The code viewers paint
 *  from these AND from `--code-background`, one surface with the terminal, so a
 *  scheme that moved the background has to move these too; otherwise it leaves
 *  the app theme's inks on someone else's paper (a light theme's dark syntax on
 *  a scheme's near-black, which is simply unreadable).
 *
 *  Deliberately the standard slots and not the bright ones: a light scheme
 *  (Solarized Light) spends its bright slots on greys, so the bright set would
 *  hand half the syntax palette the same colour. Comments are the exception -
 *  bright black IS the muted grey every palette reserves for them. */
const SYNTAX_TOKEN_SOURCES = [
  ["--syntax-comment", "--terminal-bright-black"],
  ["--syntax-keyword", "--terminal-magenta"],
  ["--syntax-string", "--terminal-green"],
  ["--syntax-number", "--terminal-yellow"],
  ["--syntax-function", "--terminal-blue"],
  ["--syntax-property", "--terminal-cyan"]
] as const satisfies readonly (readonly [string, (typeof ANSI_COLOR_TOKENS)[number]])[];

/** Every property an installed scheme writes - and therefore exactly what has
 *  to be removed again to hand the terminal back to the app theme. */
export const TERMINAL_PALETTE_PROPERTIES = [
  CODE_BACKGROUND_TOKEN,
  CODE_FOREGROUND_TOKEN,
  TERMINAL_CURSOR_TOKEN,
  TERMINAL_SELECTION_TOKEN,
  ...SYNTAX_TOKEN_SOURCES.map(([token]) => token),
  ...ANSI_COLOR_TOKENS
] as const;

/** How opaque a selection wash is when the scheme names no selection colour of
 *  its own - Windows Terminal composites its own there, so PADE derives one
 *  from the foreground rather than leaving xterm to invent a white wash. */
const DERIVED_SELECTION_ALPHA = 0.3;

const HEX_RADIX = 16;
/** Hex digits per channel in the long form (`#rrggbb`). */
const HEX_DIGITS_PER_CHANNEL = 2;
const CHANNEL_MAXIMUM = 255;
/** Below this perceived luminance a background reads as a dark terminal. Well
 *  clear of both ends in practice: Solarized Light lands near 0.9, Solarized
 *  Dark near 0.02. */
const DARK_BACKGROUND_LUMINANCE = 0.4;

function channelHex(value: number) {
  return value.toString(HEX_RADIX).padStart(HEX_DIGITS_PER_CHANNEL, "0");
}

/** The same colour with an alpha channel appended, as the `#rrggbbaa` xterm's
 *  own parser accepts (see `lib/terminal/terminal-theme` on why parse-safety matters). */
function withAlpha({ color, alpha }: {
  color: string;
  alpha: number;
}) {
  const channels = hexChannels(color);
  if (!channels) {
    return color;
  }

  return `#${channels.map(channelHex).join("")}${channelHex(Math.round(alpha * CHANNEL_MAXIMUM))}`;
}

/** Light or dark, as the terminal's own background answers it. This is what the
 *  agent is told (`theming.rs` themes each CLI from it): a Claude painting its
 *  dark theme onto Solarized Light is unreadable, and the app's own light/dark
 *  says nothing about a terminal the user gave a palette of its own. An
 *  unreadable colour falls back to `fallback` - the app's scheme. */
export function schemeOfBackground({
  color,
  fallback
}: {
  color: string;
  fallback: Scheme;
}) {
  const luminance = hexLuminance(color);
  if (luminance === null) {
    return fallback;
  }

  return luminance < DARK_BACKGROUND_LUMINANCE ? Scheme.enum.dark : Scheme.enum.light;
}

/** The custom properties this scheme installs on the document root. */
export function terminalPaletteProperties(scheme: TerminalScheme) {
  const properties = new Map<string, string>([
    [CODE_BACKGROUND_TOKEN, scheme.background],
    [CODE_FOREGROUND_TOKEN, scheme.foreground],
    [TERMINAL_CURSOR_TOKEN, scheme.cursor ?? scheme.foreground],
    [TERMINAL_SELECTION_TOKEN, scheme.selection ?? withAlpha({
      color: scheme.foreground,
      alpha: DERIVED_SELECTION_ALPHA
    })]
  ]);
  for (const [i, token] of ANSI_COLOR_TOKENS.entries()) {
    properties.set(token, scheme.ansi[i]);
  }

  for (const [token, source] of SYNTAX_TOKEN_SOURCES) {
    properties.set(token, scheme.ansi[ANSI_COLOR_TOKENS.indexOf(source)]);
  }

  return properties;
}
