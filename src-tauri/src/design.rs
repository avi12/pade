//! AI design / UI-generation tools — quick-launch from PADE.
//!
//! A parallel to the IDE menu: PADE is agent-first, but sometimes you want to
//! sketch or generate a UI in a design-to-code tool — Claude, Google Stitch,
//! Vercel v0, and peers. The roster is **tied to the active agent**: the tool
//! from the same vendor as the running agent (Claude Code → Claude, Antigravity
//! → Stitch) is surfaced first, the rest follow as general design tools. Adding
//! one is a single `REGISTRY` entry (DRY); nothing else hard-codes a product.

use serde::Serialize;
use tauri::{AppHandle, Manager, Url, WebviewUrl, WebviewWindowBuilder};

struct DesignDef {
    id: &'static str,
    label: &'static str,
    /// Vendor, shown as a subtle tag next to the name.
    vendor: &'static str,
    /// Where the tool lives (opened in the companion PADE window).
    url: &'static str,
    /// Agent ids (from `agents.rs`) this tool's vendor matches — it's pinned to
    /// the top and flagged when one of them is the active agent. Empty = a
    /// general design tool, always offered but never vendor-matched.
    agents: &'static [&'static str],
}

/// Known AI design/UI-generation products, Claude first.
const REGISTRY: &[DesignDef] = &[
    DesignDef {
        id: "claude",
        label: "Claude",
        vendor: "Anthropic",
        url: "https://claude.ai/new",
        agents: &["claude"],
    },
    DesignDef {
        id: "stitch",
        label: "Stitch",
        vendor: "Google",
        url: "https://stitch.withgoogle.com",
        agents: &["antigravity"],
    },
    DesignDef {
        id: "v0",
        label: "v0",
        vendor: "Vercel",
        url: "https://v0.app",
        agents: &[],
    },
    DesignDef {
        id: "figma-make",
        label: "Figma Make",
        vendor: "Figma",
        url: "https://www.figma.com/make",
        agents: &[],
    },
];

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DesignTool {
    id: String,
    label: String,
    vendor: String,
    url: String,
    /// True when this tool's vendor matches the active agent.
    recommended: bool,
}

/// The curated roster, ranked for the active `agent`: the vendor-matched tool is
/// pinned first (and flagged), the rest keep registry order.
#[tauri::command]
pub fn design_tools(agent: String) -> Vec<DesignTool> {
    let mut tools: Vec<DesignTool> = REGISTRY
        .iter()
        .map(|d| DesignTool {
            id: d.id.into(),
            label: d.label.into(),
            vendor: d.vendor.into(),
            url: d.url.into(),
            recommended: d.agents.contains(&agent.as_str()),
        })
        .collect();
    // Stable: recommended tool(s) float up, everything else keeps its order.
    tools.sort_by_key(|t| !t.recommended);
    tools
}

// The design tool lives in a companion PADE window.
//
// iframes are impossible here: claude.ai / stitch / v0 / figma all send
// `X-Frame-Options`, so the tool's live UI needs a *native* webview. An embedded
// child webview (Tauri `unstable` multiwebview) does NOT load external content on
// Windows/WebView2 — the create + `navigate` calls both succeed but the view
// stays on about:blank — so we host the tool in a reusable top-level PADE window
// instead. Still in-app (never the external browser), and it sits beside the main
// window as a side-by-side surface.

const DESIGN_WINDOW: &str = "design";

/// Open the companion design window on `url` — or, if it's already open, focus it
/// and navigate it to the newly picked tool.
#[tauri::command]
pub fn design_open(app: AppHandle, url: String) -> Result<(), String> {
    let target = Url::parse(&url).map_err(|e| e.to_string())?;
    if let Some(window) = app.get_webview_window(DESIGN_WINDOW) {
        window.navigate(target).map_err(|e| e.to_string())?;
        window.show().map_err(|e| e.to_string())?;
        window.set_focus().map_err(|e| e.to_string())?;
        return Ok(());
    }
    WebviewWindowBuilder::new(&app, DESIGN_WINDOW, WebviewUrl::External(target))
        .title("Design · PADE")
        .inner_size(1100.0, 820.0)
        .min_inner_size(480.0, 480.0)
        .build()
        .map_err(|e| e.to_string())?;
    Ok(())
}
