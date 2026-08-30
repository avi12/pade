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

/** Does this mode take its light/dark from the OS? "Auto" does by definition,
 *  and Cyberpunk does because it names a palette and never a scheme — so both
 *  must re-apply when the OS flips. */
export function followsSystemScheme(mode: ThemeMode): boolean {
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
  if (mode === ThemeMode.enum.cyberpunk) {
    return {
      scheme: systemScheme,
      palette: ThemePalette.enum.cyberpunk
    };
  }

  if (mode === ThemeMode.enum.system) {
    return {
      scheme: systemScheme,
      palette: ThemePalette.enum.default
    };
  }

  return {
    scheme: mode,
    palette: ThemePalette.enum.default
  };
}
