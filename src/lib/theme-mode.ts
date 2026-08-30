// How the theme the user picked resolves into what actually paints the app.
//
// There are two axes, and the picker deliberately shows them as one list: the
// light/dark `scheme` the document root wears as `data-theme`, and the token
// set — the `palette` — layered over it as `data-palette`. "Light" and "Dark"
// name a scheme; "Auto" takes the OS's; "Cyberpunk" names a palette and takes
// its scheme from the OS too, so the neon skin has a day face and a night face.
//
// Pure and testable on purpose: `lib/prefs` owns the DOM and the OS listeners,
// this owns the rule.

import { type Scheme, ThemeMode, ThemePalette } from "@/lib/types";

/** What the document root wears: the concrete scheme and the palette over it. */
export type Appearance = {
  scheme: Scheme;
  palette: ThemePalette;
};

/** The monospace face each palette leads the terminal's font stack with, on top
 *  of the stack theme.css owns. `default` adds none — that stack stands as
 *  written. The name must match the `@font-face` theme.css ships — it is the
 *  vendored woff2's family, not a hopeful guess at an installed font — and an
 *  explicit font pick still outranks it (`lib/prefs`): the palette supplies a
 *  default, not a lock. */
export const PALETTE_MONOSPACE_FACE: Record<ThemePalette, string | null> = {
  default: null,
  cyberpunk: "Share Tech Mono"
};

/** The palette a mode names — the skin axis on its own, which no mode takes
 *  from the OS. */
export function paletteOfMode(mode: ThemeMode): ThemePalette {
  if (mode === ThemeMode.enum.cyberpunk) {
    return ThemePalette.enum.cyberpunk;
  }

  return ThemePalette.enum.default;
}

/** The terminal-font pick a theme change carries with it. A skin that ships a
 *  face (Cyberpunk) adopts it, so choosing the theme visibly moves the font
 *  selection instead of quietly outranking it; leaving that skin releases the
 *  face again. A font the user chose themselves is left alone — `undefined`
 *  means "don't touch the pick".
 *
 *  Telling the two apart takes no extra state: a pick that IS some palette's
 *  face was adopted from a palette, and nothing else writes those names. */
export function adoptedMonospaceFace({
  mode,
  current
}: {
  mode: ThemeMode;
  current: string | null | undefined;
}): string | null | undefined {
  const face = PALETTE_MONOSPACE_FACE[paletteOfMode(mode)];
  if (face) {
    return face;
  }

  const wearingAPaletteFace =
    typeof current === "string" && Object.values(PALETTE_MONOSPACE_FACE).includes(current);
  if (wearingAPaletteFace) {
    return null;
  }

  return undefined;
}

/** Does this mode take its light/dark from the OS? "Auto" does by definition,
 *  and Cyberpunk does because it names a palette and never a scheme — so both
 *  must re-apply when the OS flips. Typed as a predicate because the answer is
 *  exactly "this mode is not itself a scheme": what it rules out is a `Scheme`. */
export function followsSystemScheme(mode: ThemeMode): mode is Exclude<ThemeMode, Scheme> {
  return mode === ThemeMode.enum.system || mode === ThemeMode.enum.cyberpunk;
}

/** Resolve the chosen mode against what the OS is currently showing. */
export function resolveAppearance({
  mode,
  systemScheme
}: {
  mode: ThemeMode;
  systemScheme: Scheme;
}): Appearance {
  const palette = paletteOfMode(mode);
  if (followsSystemScheme(mode)) {
    return {
      scheme: systemScheme,
      palette
    };
  }

  return {
    scheme: mode,
    palette
  };
}
