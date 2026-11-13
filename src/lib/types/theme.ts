import { z } from "zod";

import { ANSI_COLOR_TOKENS } from "@/lib/terminal/theme/terminal-theme";

/** The light/dark mode the user picks: a scheme of its own, or "system" -
 *  whatever the OS is showing. The skin over it is a separate pick, a
 *  {@link ThemePalette}; `lib/settings/theme-mode` resolves the two together. */
export const ThemeMode = z.enum(["system", "light", "dark"]);

export type ThemeMode = z.infer<typeof ThemeMode>;

/** The token set painting the app - the skin axis, picked apart from the mode,
 *  worn by the document root as `data-palette` and layered over the light/dark
 *  tokens in theme.css. */
export const ThemePalette = z.enum(["default", "cyberpunk"]);

export type ThemePalette = z.infer<typeof ThemePalette>;

/** The concrete appearance `ThemeMode` resolves to ("system" → the OS answer).
 *  Applied to the document root, the terminal palette, and - over the wire -
 *  each agent's spawn-time theme env/args (theming.rs). */
export const Scheme = z.enum(["light", "dark"]);

export type Scheme = z.infer<typeof Scheme>;

/** One Windows Terminal colour scheme, as the backend hands it over
 *  (`terminal_schemes`): the surface colours plus the 16 ANSI slots in ANSI
 *  order - the order `ANSI_COLOR_TOKENS` maps onto the `--terminal-*` design
 *  tokens, so an index here IS the SGR colour. `cursor` and `selection` are
 *  absent in several shipped schemes; `lib/terminal/theme/terminal-palette` derives those. */
export const TerminalScheme = z.object({
  name: z.string(),
  background: z.string(),
  foreground: z.string(),
  cursor: z.string().nullish(),
  selection: z.string().nullish(),
  ansi: z.array(z.string()).length(ANSI_COLOR_TOKENS.length)
});

export type TerminalScheme = z.infer<typeof TerminalScheme>;

/** The terminal colour scheme chosen for each app scheme, by name. Two slots
 *  rather than one so the terminal can follow light/dark exactly as the app
 *  does - a light scheme by day, a dark one by night. An empty slot (or a name
 *  no longer in the catalogue) means "paint from the app theme's own tokens". */
export const TerminalSchemeChoice = z.object({
  light: z.string().nullish(),
  dark: z.string().nullish()
});

export type TerminalSchemeChoice = z.infer<typeof TerminalSchemeChoice>;

export const DiffStyle = z.enum(["unified", "split"]);
