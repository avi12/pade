//! AI design / UI-generation tools — quick-launch from PADE.
//!
//! A parallel to the IDE menu: PADE is agent-first, but sometimes you want to
//! sketch or generate a UI in a design-to-code tool — Claude, Google Stitch,
//! Vercel v0, and peers. The roster is **tied to the active agent**: the tool
//! from the same vendor as the running agent (Claude Code → Claude, Antigravity
//! → Stitch) is surfaced first, the rest follow as general design tools. Adding
//! one is a single `REGISTRY` entry (DRY); nothing else hard-codes a product.

use serde::Serialize;
use tauri::webview::WebviewBuilder;
use tauri::{AppHandle, LogicalPosition, LogicalSize, Manager, Url, WebviewUrl};

struct DesignDef {
    id: &'static str,
    label: &'static str,
    /// Vendor, shown as a subtle tag next to the name.
    vendor: &'static str,
    /// Where the tool lives (opened in the default browser).
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

// Docking the design tool as a native child webview.
//
// iframes are impossible here: claude.ai / stitch / v0 / figma all send
// `X-Frame-Options`, so the tool's live UI can only live in a *native* webview.
// We host a single reusable child webview (`design-view`) on the main window,
// positioned by the frontend `DesignPanel` over the right-hand side pane. The
// panel measures its host div and passes logical bounds; we create the webview
// on first embed, then just re-`navigate`/reposition it thereafter — closing
// only parks it off-screen so the tool's session survives a reopen.

const DESIGN_WEBVIEW: &str = "design-view";

#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Bounds {
    x: f64,
    y: f64,
    width: f64,
    height: f64,
}

/// Dock `url` in the design child webview at `bounds`, creating it on first use.
#[tauri::command]
pub fn design_embed(app: AppHandle, url: String, bounds: Bounds) -> Result<(), String> {
    let target = Url::parse(&url).map_err(|e| e.to_string())?;
    if let Some(webview) = app.get_webview(DESIGN_WEBVIEW) {
        webview.navigate(target).map_err(|e| e.to_string())?;
        webview
            .set_position(LogicalPosition::new(bounds.x, bounds.y))
            .map_err(|e| e.to_string())?;
        webview
            .set_size(LogicalSize::new(bounds.width, bounds.height))
            .map_err(|e| e.to_string())?;
        return Ok(());
    }
    let window = app
        .get_window("main")
        .ok_or_else(|| "no main window".to_string())?;
    window
        .add_child(
            WebviewBuilder::new(DESIGN_WEBVIEW, WebviewUrl::External(target)),
            LogicalPosition::new(bounds.x, bounds.y),
            LogicalSize::new(bounds.width, bounds.height),
        )
        .map_err(|e| e.to_string())?;
    Ok(())
}

/// Reposition the design webview as its host pane moves/resizes (no navigation).
#[tauri::command]
pub fn design_set_bounds(app: AppHandle, bounds: Bounds) -> Result<(), String> {
    if let Some(webview) = app.get_webview(DESIGN_WEBVIEW) {
        webview
            .set_position(LogicalPosition::new(bounds.x, bounds.y))
            .map_err(|e| e.to_string())?;
        webview
            .set_size(LogicalSize::new(bounds.width, bounds.height))
            .map_err(|e| e.to_string())?;
    }
    Ok(())
}

/// Hide the design webview by parking it off-screen — keeps the tool's session.
#[tauri::command]
pub fn design_close(app: AppHandle) -> Result<(), String> {
    if let Some(webview) = app.get_webview(DESIGN_WEBVIEW) {
        webview
            .set_position(LogicalPosition::new(0.0, 100_000.0))
            .map_err(|e| e.to_string())?;
    }
    Ok(())
}
