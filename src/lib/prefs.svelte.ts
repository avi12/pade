// Reactive appearance/editor preferences, shared app-wide. Loaded once from the
// persisted settings, applied to the document root as CSS custom properties and
// a theme-mode data attribute, and saved back through the bridge.

import { workspace } from "@/lib/bridge";
import { DEFAULT_CONTEXT_HANDOFF_PERCENTAGE } from "@/lib/context-level";
import { SIDE_PANEL_DEFAULT_WIDTH } from "@/lib/prefs-bounds";
import { prefs, registerSettingsAdoptionEffect } from "@/lib/settings.svelte";
import type { DiffStyle, Prefs, Scheme, ThemeMode } from "@/lib/types";

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
  }
};

const osDark = matchMedia("(prefers-color-scheme: dark)");

/** The concrete scheme currently applied — reactive so consumers like the
 *  terminal can re-theme when it changes. */
export const appearance = $state<{ scheme: Scheme }>({ scheme: osDark.matches ? "dark" : "light" });

/** Resolve "system" to the concrete scheme so the CSS needs only one dark block. */
function resolvedScheme(): Scheme {
  const mode = effective.themeMode;
  if (mode === "system") {
    return osDark.matches ? "dark" : "light";
  }

  return mode;
}

function apply() {
  // Fonts are bound declaratively in the template (style:--font-ui / --font-monospace).
  // Theme mode stays here: it must sit on <html> for the pre-paint flash guard
  // and to cover anything rendered outside the app root.
  appearance.scheme = resolvedScheme();
  document.documentElement.dataset.theme = appearance.scheme;
  // Font scaling follows video-time-manager: the root font is `100% * --ui-scale`
  // (the user's browser base, times their zoom preference — never a fixed px that
  // would override OS/browser a11y sizing), and `--font-base` (theme.css) derives a
  // ≥16px unit from it. rem/em UI and the terminal scale from the one knob.
  document.documentElement.style.setProperty("--ui-scale", String(effective.uiScale));
}

// Re-apply when the OS theme flips while we're following it.
osDark.addEventListener("change", () => {
  if (effective.themeMode === "system") {
    apply();
  }
});

registerSettingsAdoptionEffect(apply);

export async function loadPrefs(): Promise<void> {
  await workspace.settings();
}

/** Merge a change, apply it immediately, then persist. */
export async function updatePrefs(patch: Partial<Prefs>): Promise<void> {
  Object.assign(prefs, patch);
  apply();
  await workspace.setPrefs(patch);
}
