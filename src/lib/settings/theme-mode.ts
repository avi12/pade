// How the theme the user picked resolves into what actually paints the app.
//
// Two axes, picked apart: the light/dark `mode` - "Light", "Dark", or "Auto",
// which takes the OS's - that the document root wears as `data-theme`, and the
// token set, the `palette` or skin, layered over it as `data-palette`. Every
// skin works with every mode, so the Cyberpunk skin has a day face and a night
// face.
//
// Pure and testable on purpose: `lib/settings/prefs` owns the DOM and the OS listeners,
// this owns the rule.

import { type Scheme, ThemeMode, ThemePalette } from "@/lib/types";

/** What the document root wears: the concrete scheme and the palette over it. */
type Appearance = {
  scheme: Scheme;
  palette: ThemePalette;
};

/** The monospace face each palette leads the terminal's font stack with, on top
 *  of the stack theme.css owns. `default` adds none - that stack stands as
 *  written. The name must match the `@font-face` theme.css ships - it is the
 *  vendored woff2's family, not a hopeful guess at an installed font - and an
 *  explicit font pick still outranks it (`lib/settings/prefs`): the palette supplies a
 *  default, not a lock. */
export const PALETTE_MONOSPACE_FACE: Record<ThemePalette, string | null> = {
  [ThemePalette.enum.default]: null,
  [ThemePalette.enum.cyberpunk]: "Share Tech Mono"
};

/** The terminal-font pick a skin change carries with it. A skin that ships a
 *  face (Cyberpunk) adopts it, so choosing the skin visibly moves the font
 *  selection instead of quietly outranking it; leaving that skin releases the
 *  face again. A font the user chose themselves is left alone - `undefined`
 *  means "don't touch the pick".
 *
 *  Telling the two apart takes no extra state: a pick that IS some palette's
 *  face was adopted from a palette, and nothing else writes those names. */
export function adoptedMonospaceFace({
  palette,
  current
}: {
  palette: ThemePalette;
  current: string | null | undefined;
}) {
  const face = PALETTE_MONOSPACE_FACE[palette];
  if (face) {
    return face;
  }

  const isWearingPaletteFace =
    typeof current === "string" && Object.values(PALETTE_MONOSPACE_FACE).includes(current);
  if (isWearingPaletteFace) {
    return null;
  }

  return undefined;
}

/** Does this mode take its light/dark from the OS? Only "Auto" does, so only it
 *  must re-apply when the OS flips. Typed as a predicate because the answer is
 *  exactly "this mode is not itself a scheme": what it rules out is a `Scheme`. */
export function followsSystemScheme(mode: ThemeMode): mode is Exclude<ThemeMode, Scheme> {
  return mode === ThemeMode.enum.system;
}

/** Resolve the chosen mode and skin against what the OS is currently showing. */
export function resolveAppearance({
  mode,
  palette,
  systemScheme
}: {
  mode: ThemeMode;
  palette: ThemePalette;
  systemScheme: Scheme;
}) {
  if (followsSystemScheme(mode)) {
    return {
      scheme: systemScheme,
      palette
    } satisfies Appearance;
  }

  return {
    scheme: mode,
    palette
  } satisfies Appearance;
}
