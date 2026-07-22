// Reactive appearance/editor preferences, shared app-wide. Loaded once from the
// persisted settings, applied to the document root as CSS custom properties and
// a theme-mode data attribute, and saved back through the bridge.

import { workspace } from "@/lib/bridge";
import { DEFAULT_CONTEXT_HANDOFF_PCT } from "@/lib/context-level";
import type { DiffStyle, Prefs, Scheme, ThemeMode } from "@/lib/types";

const UI_FALLBACK = "\"Google Sans\", \"Segoe UI\", system-ui, sans-serif";
const MONO_FALLBACK = "\"JetBrains Mono\", \"Cascadia Code\", ui-monospace, monospace";

/** Default side-panel width when the user hasn't dragged the divider. Matches the
 *  design mockup's `panelW:380` (and its double-click reset). */
export const SIDE_PANEL_DEFAULT_WIDTH = 380;
/** Smallest usable side-panel width; the live clamp also caps the max at 60% of the
 *  window so the terminal is never swallowed. */
export const SIDE_PANEL_MIN_WIDTH = 280;

export const prefs = $state<Prefs>({});

/** Effective values with defaults resolved (for consumers that need a concrete value). */
export const effective = {
  get themeMode(): ThemeMode {
    return prefs.themeMode ?? "system";
  },
  get diffStyle(): DiffStyle {
    return prefs.diffStyle ?? "unified";
  },
  get monoFamily(): string {
    return prefs.monoFont ? `"${prefs.monoFont}", ${MONO_FALLBACK}` : MONO_FALLBACK;
  },
  get uiFamily(): string {
    return prefs.uiFont ? `"${prefs.uiFont}", ${UI_FALLBACK}` : UI_FALLBACK;
  },
  get uiScale(): number {
    return prefs.uiScale ?? 1;
  },
  get sidePanelWidth(): number {
    return prefs.sidePanelWidth ?? SIDE_PANEL_DEFAULT_WIDTH;
  },
  get handoffPct(): number {
    return prefs.handoffPct ?? DEFAULT_CONTEXT_HANDOFF_PCT;
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
  // Font scaling follows youtube-time-manager: the root font is `100% * --ui-scale`
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

/** Adopt preferences the backend just persisted (a settings load, a saved
 *  patch, an editor pick from another store). Every backend write that returns
 *  fresh `Settings` must funnel its `prefs` through here: `set_prefs` replaces
 *  the whole set with this store's copy, so a key missing here (e.g. a
 *  just-persisted `ideProjectChoices` entry) would be erased by the next save. */
export function adoptPrefs(fresh: Prefs): void {
  Object.assign(prefs, fresh);
  apply();
}

export async function loadPrefs(): Promise<void> {
  const settings = await workspace.settings();
  adoptPrefs(settings.prefs);
}

/** Merge a change, apply it immediately, then persist. */
export async function updatePrefs(patch: Partial<Prefs>): Promise<void> {
  Object.assign(prefs, patch);
  apply();
  const settings = await workspace.setPrefs(prefs);
  adoptPrefs(settings.prefs);
}
