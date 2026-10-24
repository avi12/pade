// Reactive appearance/editor preferences, shared app-wide. Loaded once from the
// persisted settings, applied to the document root as CSS custom properties and
// a theme-mode data attribute, and saved back through `set_prefs`.

import { Command, EventType, onMessage, sendMessage } from "@/lib/ipc/messaging";
import { SIDE_PANEL_DEFAULT_WIDTH } from "@/lib/settings/prefs-bounds";
import { sendSettingsMessage } from "@/lib/settings/settings-requests";
import { prefs, registerSettingsAdoptionEffect } from "@/lib/settings/settings.svelte";
import { adoptedMonospaceFace, followsSystemScheme, resolveAppearance } from "@/lib/settings/theme-mode";
import { chosenTerminalScheme, loadTerminalSchemes } from "@/lib/stores/ui/terminalSchemes.svelte";
import {
  schemeOfBackground,
  TERMINAL_PALETTE_PROPERTIES,
  terminalPaletteProperties
} from "@/lib/terminal/terminal-palette";
import {
  DiffStyle,
  type Prefs,
  Scheme,
  type TerminalSchemeChoice,
  ThemeMode,
  ThemePalette
} from "@/lib/types";
import { DEFAULT_CONTEXT_HANDOFF_PERCENTAGE } from "@/lib/usage/context-level";

export {
  SIDE_PANEL_DEFAULT_WIDTH,
  SIDE_PANEL_MAXIMUM_FRACTION,
  SIDE_PANEL_MINIMUM_WIDTH
} from "@/lib/settings/prefs-bounds";

/** Resolve a font-stack CSS custom property (theme.css owns the fallback list) to
 *  its concrete value and prepend the user's chosen font when set. */
function withChosenFont({ chosen, baseVariable }: {
  chosen: string | null | undefined;
  baseVariable: string;
}) {
  const base = getComputedStyle(document.documentElement).getPropertyValue(baseVariable).trim();
  return chosen ? `"${chosen}", ${base}` : base;
}

const DEFAULT_UI_SCALE = 1;
const UI_SCALE_PROPERTY = "--ui-scale";

function schemeOf(isDark: boolean) {
  return isDark ? Scheme.enum.dark : Scheme.enum.light;
}

/** Nothing chosen on either side - the terminal follows the app theme. */
const NO_TERMINAL_SCHEMES: TerminalSchemeChoice = {
  light: null,
  dark: null
};

/** Effective values with defaults resolved (for consumers that need a concrete value). */
export const effective = {
  get themeMode() {
    return prefs.themeMode ?? ThemeMode.enum.system;
  },
  get diffStyle() {
    return prefs.diffStyle ?? DiffStyle.enum.unified;
  },
  /** The terminal font pick, as a name; "" = theme.css's stack as written. */
  get monoFontName() {
    return prefs.monoFont ?? "";
  },
  get monospaceFamily() {
    // The base stack lives once in theme.css (--font-monospace-base); resolve it to
    // a concrete string (xterm renders to canvas and can't resolve a `var()`) and
    // prepend a chosen font, so the fallback list is never re-spelled here.
    return withChosenFont({
      chosen: prefs.monoFont,
      baseVariable: "--font-monospace-base"
    });
  },
  /** The UI stack for CSS: a chosen face in front of the theme's own stack, left
   *  as a `var()` rather than resolved - nothing paints UI text to a canvas, and
   *  keeping the reference live is what lets a palette that brings its own face
   *  (Cyberpunk is set in Rajdhani, the game's) take effect without this getter
   *  re-running. `null` when nothing is chosen: the root's `--font-ui` already
   *  IS that stack, so the binding is simply dropped. */
  get uiFamily() {
    if (!prefs.uiFont) {
      return null;
    }

    return `"${prefs.uiFont}", var(--font-ui-base)`;
  },
  get uiScale() {
    return prefs.uiScale ?? DEFAULT_UI_SCALE;
  },
  /** The Windows Terminal colour scheme chosen per app scheme (`lib/stores/
   *  terminalSchemes` resolves a name to the scheme itself). */
  get terminalSchemes() {
    return prefs.terminalSchemes ?? NO_TERMINAL_SCHEMES;
  },
  get sidePanelWidth() {
    return prefs.sidePanelWidth ?? SIDE_PANEL_DEFAULT_WIDTH;
  },
  get handoffPercentage() {
    return prefs.handoffPct ?? DEFAULT_CONTEXT_HANDOFF_PERCENTAGE;
  },
  get softwareRender() {
    return prefs.softwareRender === true;
  },
  /** Opt-in: nothing is published to Discord unless explicitly enabled. */
  get discordPresence() {
    return prefs.discordPresence === true;
  },
  /** On by default, but only reachable once presence itself is on. */
  get discordShowProject() {
    return prefs.discordShowProject !== false;
  },
  /** Opt-out: PADE checks GitHub for a newer release unless turned off. */
  get autoUpdate() {
    return prefs.autoUpdate !== false;
  }
};

/** The media query that answers whether the OS is showing dark. */
export const DARK_SCHEME_QUERY = "(prefers-color-scheme: dark)";
const darkSchemeQuery = matchMedia(DARK_SCHEME_QUERY);

/** What the OS is showing, in one place. Seeded from the native window theme at
 *  startup and moved only by the two OS signals (the media query, the native flip
 *  event) - every "is the system dark?" question resolves through this, never by
 *  re-reading `darkSchemeQuery.matches` at the point of use.
 *
 *  The seed matters: WebView2 can answer `prefers-color-scheme: light` in a window
 *  that is already dark, because it resolves after the first script runs. Whoever
 *  read the media query in that window got "light" and nothing ever corrected it -
 *  the flip event only fires on a genuine change. A session spawned in that gap
 *  themed its agent light for the whole life of the process (Claude reads
 *  `$COLORFGBG` once, at spawn). */
let systemScheme: Scheme = schemeOf(darkSchemeQuery.matches);

/** What is currently painted - the concrete light/dark scheme and the palette
 *  layered over it - reactive so consumers like the terminal can re-theme when
 *  either moves. `lib/settings/theme-mode` owns the mode → pair rule. */
export const appearance = $state<{
  scheme: Scheme;
  palette: ThemePalette;
  /** The name of the Windows Terminal scheme painting the terminal, or `null`
   *  when it follows the app theme. Reading it is what re-themes xterm when the
   *  user picks another one. */
  terminalPalette: string | null;
  /** Light or dark as the TERMINAL is painted - the chosen scheme's own
   *  background when there is one, else the app's. This, never `scheme`, is what
   *  an agent is told: it paints into the terminal, not into the app chrome. */
  terminalScheme: Scheme;
}>({
  scheme: systemScheme,
  palette: ThemePalette.enum.default,
  terminalPalette: null,
  terminalScheme: systemScheme
});

/** Apply a resolved appearance to the reactive store and the `<html>` data
 *  attributes in one place - the single home for "what is painted right now",
 *  used by both the media-query path and the native OS-theme event. */
function installAppearance({
  scheme,
  palette
}: {
  scheme: Scheme;
  palette: ThemePalette;
}) {
  appearance.scheme = scheme;
  appearance.palette = palette;
  document.documentElement.dataset.theme = scheme;
  document.documentElement.dataset.palette = palette;
}

/** Paint the terminal from the scheme chosen for `scheme`, or hand it back to
 *  the app theme's own `--terminal-*` tokens when there is none. Writing the
 *  tokens rather than talking to xterm is what keeps every ANSI surface (the
 *  terminal, the runner dock) on one palette. */
function installTerminalPalette(scheme: Scheme) {
  const chosen = chosenTerminalScheme({
    choice: prefs.terminalSchemes,
    scheme
  });
  const root = document.documentElement.style;
  if (!chosen) {
    for (const property of TERMINAL_PALETTE_PROPERTIES) {
      root.removeProperty(property);
    }

    appearance.terminalPalette = null;
    appearance.terminalScheme = scheme;
    return;
  }

  for (const [property, color] of terminalPaletteProperties(chosen)) {
    root.setProperty(property, color);
  }

  appearance.terminalPalette = chosen.name;
  appearance.terminalScheme = schemeOfBackground({
    color: chosen.background,
    fallback: scheme
  });
}

function apply() {
  // Fonts are bound declaratively in the template (style:--font-ui / --font-monospace).
  // Theme mode stays here: it must sit on <html> for the pre-paint flash guard
  // and to cover anything rendered outside the app root.
  installAppearance(
    resolveAppearance({
      mode: effective.themeMode,
      systemScheme
    })
  );
  // Font scaling follows video-time-manager: the root font is `100% * --ui-scale`
  // (the user's browser base, times their zoom preference - never a fixed px that
  // would override OS/browser a11y sizing), and `--font-base` (theme.css) derives a
  // ≥16px unit from it. rem/em UI and the terminal scale from the one knob.
  document.documentElement.style.setProperty(UI_SCALE_PROPERTY, String(effective.uiScale));
  installTerminalPalette(appearance.scheme);
}

/** Record what the OS is showing and re-apply if we're following it. The scheme
 *  is recorded even on a fixed light/dark mode, so switching back to "system"
 *  lands on the OS's current answer rather than a stale one. */
function installSystemScheme(scheme: Scheme) {
  systemScheme = scheme;

  if (followsSystemScheme(effective.themeMode)) {
    apply();
  }
}

// Re-apply when the OS theme flips while we're following it.
darkSchemeQuery.addEventListener("change", e => {
  installSystemScheme(schemeOf(e.matches));
});

// A PADE window parked on another virtual desktop (or minimized) is hidden, and
// Chromium freezes a hidden page: the `change` above never fires for an OS flip
// made while it was away, and it is NOT re-fired on thaw - so the scheme silently
// goes stale and the agent keeps painting the old one. Reconcile on the way back
// to visible by re-reading the media query live, so a missed flip re-themes xterm
// and relays the ?997 report through the terminal's own effects. Keyed on
// visibility, not focus, so it fires the instant the desktop is switched back.
document.addEventListener("visibilitychange", () => {
  if (document.visibilityState === "visible") {
    installSystemScheme(schemeOf(darkSchemeQuery.matches));
  }
});

registerSettingsAdoptionEffect(apply);

let isSystemThemeWatched = false;

/** Follow the native OS theme flip. WebView2 does not reliably deliver the
 *  `prefers-color-scheme` `change` to a window that stays focused/visible, so the
 *  media-query listener (and the visibility reconcile, which only covers a window
 *  returning to view) can both miss it - the terminal then keeps the old scheme
 *  with the window focused the whole time. Tauri's native `ThemeChanged` fires
 *  regardless; follow the scheme IT reports (authoritative - the media query's own
 *  value can still be stale) so a focused window re-themes at once. Armed once. */
async function watchSystemTheme() {
  if (isSystemThemeWatched) {
    return;
  }

  isSystemThemeWatched = true;
  await onMessage(EventType.themeChanged, ({ data }) => {
    installSystemScheme(schemeOf(data === Scheme.enum.dark));
  });
}

/** Seed the OS scheme from the native window before anything reads it. The media
 *  query alone is not enough at startup: WebView2 can answer "light" in a window
 *  that is already dark, and only a genuine flip would ever correct it - so the
 *  first session spawned would theme its agent for the wrong scheme permanently.
 *  Best-effort: a failed probe keeps the media query's answer. */
async function adoptNativeSystemScheme() {
  try {
    installSystemScheme(await sendMessage(Command.windowTheme));
  } catch (error) {
    // The media-query seed above stands, and a later flip still corrects it.
    console.debug("pade: native window theme unavailable", error);
  }
}

export async function loadPrefs() {
  await watchSystemTheme();
  await adoptNativeSystemScheme();
  // Before the settings land, so the first apply() already resolves a persisted
  // terminal scheme instead of flashing the app palette first.
  await loadTerminalSchemes();
  await sendSettingsMessage(Command.settingsGet);
}

/** Adopt a theme, taking its terminal face with it. A skin that ships a font
 *  (Cyberpunk) sets it as the font pick rather than quietly outranking one, so
 *  the change shows up in the font grid and stays the user's to change
 *  afterwards; leaving that skin releases the face again. `lib/settings/theme-mode` owns
 *  the rule, including when a pick is the user's own and must be left alone. */
export async function chooseThemeMode(mode: ThemeMode) {
  const face = adoptedMonospaceFace({
    mode,
    current: prefs.monoFont
  });
  const patch: Partial<Prefs> = { themeMode: mode };
  if (face !== undefined) {
    patch.monoFont = face;
  }

  await updatePrefs(patch);
}

/** Merge a change, apply it immediately, then persist. */
export async function updatePrefs(patch: Partial<Prefs>) {
  Object.assign(prefs, patch);
  apply();
  await sendSettingsMessage(Command.setPrefs, { patch });
}
