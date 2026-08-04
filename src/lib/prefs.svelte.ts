// Reactive appearance/editor preferences, shared app-wide. Loaded once from the
// persisted settings, applied to the document root as CSS custom properties and
// a theme-mode data attribute, and saved back through the bridge.

import { windows, workspace } from "@/lib/bridge";
import { DEFAULT_CONTEXT_HANDOFF_PERCENTAGE } from "@/lib/context-level";
import { SIDE_PANEL_DEFAULT_WIDTH } from "@/lib/prefs-bounds";
import { prefs, registerSettingsAdoptionEffect } from "@/lib/settings.svelte";
import type { DiffStyle, Prefs, Scheme, ThemeMode } from "@/lib/types";
import { listen } from "@tauri-apps/api/event";

export { prefs } from "@/lib/settings.svelte";
export {
  SIDE_PANEL_DEFAULT_WIDTH,
  SIDE_PANEL_MAX_FRACTION,
  SIDE_PANEL_MIN_WIDTH
} from "@/lib/prefs-bounds";

/** Resolve a font-stack CSS custom property (theme.css owns the fallback list) to
 *  its concrete value and prepend the user's chosen font when set. */
function withChosenFont(chosen: string | null | undefined, baseVariable: string): string {
  const base = getComputedStyle(document.documentElement).getPropertyValue(baseVariable).trim();
  return chosen ? `"${chosen}", ${base}` : base;
}

/** Effective values with defaults resolved (for consumers that need a concrete value). */
export const effective = {
  get themeMode(): ThemeMode {
    return prefs.themeMode ?? "system";
  },
  get diffStyle(): DiffStyle {
    return prefs.diffStyle ?? "unified";
  },
  get monospaceFamily(): string {
    // The base stack lives once in theme.css (--font-monospace-base); resolve it to
    // a concrete string (xterm renders to canvas and can't resolve a `var()`) and
    // prepend a chosen font, so the fallback list is never re-spelled here.
    return withChosenFont(prefs.monoFont, "--font-monospace-base");
  },
  get uiFamily(): string {
    return withChosenFont(prefs.uiFont, "--font-ui-base");
  },
  get uiScale(): number {
    return prefs.uiScale ?? 1;
  },
  get sidePanelWidth(): number {
    return prefs.sidePanelWidth ?? SIDE_PANEL_DEFAULT_WIDTH;
  },
  get handoffPercentage(): number {
    return prefs.handoffPct ?? DEFAULT_CONTEXT_HANDOFF_PERCENTAGE;
  },
  get softwareRender(): boolean {
    return prefs.softwareRender === true;
  }
};

const osDark = matchMedia("(prefers-color-scheme: dark)");

/** What the OS is showing, in one place. Seeded from the native window theme at
 *  startup and moved only by the two OS signals (the media query, the native flip
 *  event) — every "is the system dark?" question resolves through this, never by
 *  re-reading `osDark.matches` at the point of use.
 *
 *  The seed matters: WebView2 can answer `prefers-color-scheme: light` in a window
 *  that is already dark, because it resolves after the first script runs. Whoever
 *  read the media query in that window got "light" and nothing ever corrected it —
 *  the flip event only fires on a genuine change. A session spawned in that gap
 *  themed its agent light for the whole life of the process (Claude reads
 *  `$COLORFGBG` once, at spawn). */
let systemScheme: Scheme = osDark.matches ? "dark" : "light";

/** The concrete scheme currently applied — reactive so consumers like the
 *  terminal can re-theme when it changes. */
export const appearance = $state<{ scheme: Scheme }>({ scheme: systemScheme });

/** Resolve "system" to the concrete scheme so the CSS needs only one dark block. */
function resolvedScheme(): Scheme {
  const mode = effective.themeMode;
  if (mode === "system") {
    return systemScheme;
  }

  return mode;
}

/** Apply a concrete scheme to the reactive store and the `<html>` data-theme in one
 *  place — the single home for "the applied scheme is X", used by both the media-
 *  query path and the native OS-theme event. */
function installScheme(scheme: Scheme): void {
  appearance.scheme = scheme;
  document.documentElement.dataset.theme = scheme;
}

function apply() {
  // Fonts are bound declaratively in the template (style:--font-ui / --font-monospace).
  // Theme mode stays here: it must sit on <html> for the pre-paint flash guard
  // and to cover anything rendered outside the app root.
  installScheme(resolvedScheme());
  // Font scaling follows video-time-manager: the root font is `100% * --ui-scale`
  // (the user's browser base, times their zoom preference — never a fixed px that
  // would override OS/browser a11y sizing), and `--font-base` (theme.css) derives a
  // ≥16px unit from it. rem/em UI and the terminal scale from the one knob.
  document.documentElement.style.setProperty("--ui-scale", String(effective.uiScale));
}

/** Record what the OS is showing and re-apply if we're following it. The scheme
 *  is recorded even on a fixed light/dark mode, so switching back to "system"
 *  lands on the OS's current answer rather than a stale one. */
function installSystemScheme(scheme: Scheme): void {
  systemScheme = scheme;
  if (effective.themeMode === "system") {
    apply();
  }
}

// Re-apply when the OS theme flips while we're following it.
osDark.addEventListener("change", event => {
  installSystemScheme(event.matches ? "dark" : "light");
});

// A PADE window parked on another virtual desktop (or minimized) is hidden, and
// Chromium freezes a hidden page: the `change` above never fires for an OS flip
// made while it was away, and it is NOT re-fired on thaw — so the scheme silently
// goes stale and the agent keeps painting the old one. Reconcile on the way back
// to visible by re-reading the media query live, so a missed flip re-themes xterm
// and relays the ?997 report through the terminal's own effects. Keyed on
// visibility, not focus, so it fires the instant the desktop is switched back.
document.addEventListener("visibilitychange", () => {
  if (document.visibilityState === "visible") {
    installSystemScheme(osDark.matches ? "dark" : "light");
  }
});

registerSettingsAdoptionEffect(apply);

/** Wire the shared name for the backend's OS-theme event (see `window.rs`
 *  `THEME_CHANGED_EVENT`). Change both together. */
const OS_THEME_CHANGED_EVENT = "theme://changed";
let systemThemeWatched = false;

/** Follow the native OS theme flip. WebView2 does not reliably deliver the
 *  `prefers-color-scheme` `change` to a window that stays focused/visible, so the
 *  media-query listener (and the visibility reconcile, which only covers a window
 *  returning to view) can both miss it — the terminal then keeps the old scheme
 *  with the window focused the whole time. Tauri's native `ThemeChanged` fires
 *  regardless; follow the scheme IT reports (authoritative — the media query's own
 *  value can still be stale) so a focused window re-themes at once. Armed once. */
async function watchSystemTheme(): Promise<void> {
  if (systemThemeWatched) {
    return;
  }

  systemThemeWatched = true;
  await listen<string>(OS_THEME_CHANGED_EVENT, event => {
    installSystemScheme(event.payload === "dark" ? "dark" : "light");
  });
}

/** Seed the OS scheme from the native window before anything reads it. The media
 *  query alone is not enough at startup: WebView2 can answer "light" in a window
 *  that is already dark, and only a genuine flip would ever correct it — so the
 *  first session spawned would theme its agent for the wrong scheme permanently.
 *  Best-effort: a failed probe keeps the media query's answer. */
async function adoptNativeSystemScheme(): Promise<void> {
  try {
    installSystemScheme(await windows.theme());
  } catch {
    // Native theme unavailable (an older backend, a probe failure) — the
    // media-query seed above stands, and a later flip still corrects it.
  }
}

export async function loadPrefs(): Promise<void> {
  await watchSystemTheme();
  await adoptNativeSystemScheme();
  await workspace.settings();
}

/** Merge a change, apply it immediately, then persist. */
export async function updatePrefs(patch: Partial<Prefs>): Promise<void> {
  Object.assign(prefs, patch);
  apply();
  await workspace.setPrefs(patch);
}
