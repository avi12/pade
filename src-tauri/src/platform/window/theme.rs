//! A window's look: the in-theme surface it is painted with at creation, and the scheme the frontend starts on and flips to.

use super::THEME_CHANGED_EVENT;
use crate::terminal::theming::Scheme;
use tauri::window::Color;
use tauri::Emitter;
use tauri::{Manager, Theme, WebviewWindow, Window};

/// M3 surface colors, mirroring `--surface` in `src/theme.css` for the light and
/// dark schemes. Painted as the webview background at window creation so a window
/// opens already in-theme: `WebView2` otherwise shows an unthemed white surface
/// until the HTML/CSS first paints, flashing white on a dark desktop. This is the
/// native side of the token - Rust can't read the CSS custom property - so keep
/// the two in sync.
const SURFACE_LIGHT: Color = Color(248, 250, 251, 255); // hsl(210deg 30% 98%)
const SURFACE_DARK: Color = Color(14, 20, 27, 255); // hsl(214deg 30% 8%)

/// The frontend's `appearance.scheme` for a resolved OS theme. The one mapping
/// the startup read, the flip event and the surface paint all go through, so a
/// window can never start on one scheme and flip to another.
fn scheme_for(theme: Theme) -> Scheme {
    match theme {
        Theme::Dark => Scheme::Dark,
        // `Theme` is `#[non_exhaustive]`; treat light and anything new as light.
        _ => Scheme::Light,
    }
}

/// The surface color matching a resolved OS theme.
pub(super) fn surface_for(theme: Theme) -> Color {
    match scheme_for(theme) {
        Scheme::Light => SURFACE_LIGHT,
        Scheme::Dark => SURFACE_DARK,
    }
}

/// Paint `window`'s webview with the themed surface so it shows in-theme before
/// the frontend renders. Best-effort - a failed theme probe leaves the default.
pub(crate) fn paint_surface(window: &WebviewWindow) {
    if let Ok(theme) = window.theme() {
        let _ = window.set_background_color(Some(surface_for(theme)));
    }
}

/// This window's OS theme right now, for the frontend to seed its scheme with at
/// startup.
///
/// `WebView2`'s `prefers-color-scheme` can still answer *light* while the window
/// is already dark - it resolves after the first script runs - and the flip event
/// above only fires on a genuine *change*, so nothing ever corrects that first
/// answer. A session spawned in that window then themes its agent for the wrong
/// scheme for the life of the process (Claude reads `$COLORFGBG` at spawn; see
/// `theming`). Reading the native theme closes the gap. Pure registry lookup, so
/// it stays sync.
#[tauri::command]
pub(crate) fn window_theme(window: Window) -> Scheme {
    window.theme().map_or(Scheme::Light, scheme_for)
}

/// React to a runtime OS theme flip. `WebView2` does not reliably deliver the
/// `prefers-color-scheme` change to a window that stays *focused/visible* - the
/// frontend's media query can go stale with no visibility/focus transition to
/// reconcile off - so the native `ThemeChanged` window event is the trustworthy
/// signal. Repaint the surface in-theme and hand the resolved scheme to that
/// window's frontend, which follows it verbatim (see prefs.svelte). Emitted only
/// to the window that changed, so each window re-themes from its own OS theme.
pub(crate) fn on_theme_changed(window: &Window, theme: Theme) {
    if let Some(webview) = window.get_webview_window(window.label()) {
        let _ = webview.set_background_color(Some(surface_for(theme)));
    }

    let _ = window.emit_to(window.label(), THEME_CHANGED_EVENT, scheme_for(theme));
}
