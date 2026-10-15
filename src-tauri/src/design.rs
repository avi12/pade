//! AI design / UI-generation tools - quick-launch from PADE.
//!
//! A parallel to the IDE menu: PADE is agent-first, but sometimes you want to
//! sketch or generate a UI in a design-to-code tool - Claude, Google Stitch,
//! Vercel v0, and peers. The roster is **tied to the active agent**: the tool
//! from the same vendor as the running agent (Claude Code → Claude, Antigravity
//! → Stitch) is surfaced first, the rest follow as general design tools. Adding
//! one is a single `REGISTRY` entry (DRY); nothing else hard-codes a product.

use serde::Serialize;
use tauri::{AppHandle, Manager, Url, WebviewUrl, WebviewWindowBuilder};

use crate::agents::{ID_ANTIGRAVITY, ID_CLAUDE};

struct DesignDefinition {
    id: &'static str,
    label: &'static str,
    /// Vendor, shown as a subtle tag next to the name.
    vendor: &'static str,
    /// Where the tool lives (opened in the companion PADE window).
    url: &'static str,
    /// Agent ids (from `agents.rs`) this tool's vendor matches - it's pinned to
    /// the top and flagged when one of them is the active agent. Empty = a
    /// general design tool, always offered but never vendor-matched.
    agents: &'static [&'static str],
}

/// Known AI design/UI-generation products, Claude first.
const REGISTRY: &[DesignDefinition] = &[
    DesignDefinition {
        id: "claude",
        label: "Claude",
        vendor: "Anthropic",
        url: "https://claude.ai/new",
        agents: &[ID_CLAUDE],
    },
    DesignDefinition {
        id: "stitch",
        label: "Stitch",
        vendor: "Google",
        url: "https://stitch.withgoogle.com",
        agents: &[ID_ANTIGRAVITY],
    },
    DesignDefinition {
        id: "v0",
        label: "v0",
        vendor: "Vercel",
        url: "https://v0.app",
        agents: &[],
    },
    DesignDefinition {
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
    #[serde(rename = "recommended")]
    is_recommended: bool,
}

/// The curated roster, ranked for the active `agent`: the vendor-matched tool is
/// pinned first (and flagged), the rest keep registry order.
#[tauri::command]
pub fn design_tools(agent: String) -> Vec<DesignTool> {
    let mut tools: Vec<DesignTool> = REGISTRY
        .iter()
        .map(|definition| DesignTool {
            id: definition.id.into(),
            label: definition.label.into(),
            vendor: definition.vendor.into(),
            url: definition.url.into(),
            is_recommended: definition.agents.contains(&agent.as_str()),
        })
        .collect();
    // Stable: recommended tool(s) float up, everything else keeps its order.
    tools.sort_by_key(|tool| !tool.is_recommended);
    tools
}

#[cfg(test)]
mod tests {
    use super::design_tools;
    use crate::agents::{ID_ANTIGRAVITY, ID_CODEX};

    #[test]
    fn the_active_agents_vendor_tool_is_pinned_first_and_flagged() {
        let tools = design_tools(ID_ANTIGRAVITY.to_string());
        assert_eq!(tools[0].id, "stitch");
        assert!(tools[0].is_recommended);
        assert!(tools[1..].iter().all(|tool| !tool.is_recommended));

        let wire = serde_json::to_value(&tools[0]).expect("serializes");
        assert_eq!(wire["recommended"], serde_json::json!(true));
    }

    #[test]
    fn an_agent_with_no_vendor_tool_keeps_registry_order() {
        let ids: Vec<String> = design_tools(ID_CODEX.to_string())
            .into_iter()
            .map(|tool| tool.id)
            .collect();
        assert_eq!(ids, ["claude", "stitch", "v0", "figma-make"]);
    }
}

// The design tool lives in a companion PADE window.
//
// iframes are impossible here: claude.ai / stitch / v0 / figma all send
// `X-Frame-Options`, so the tool's live UI needs a *native* webview. An embedded
// child webview (Tauri `unstable` multiwebview) does NOT load external content on
// Windows/WebView2 - the create + `navigate` calls both succeed but the view
// stays on about:blank - so we host the tool in a reusable top-level PADE window
// instead. Still in-app (never the external browser), and it sits beside the main
// window as a side-by-side surface.

const DESIGN_WINDOW: &str = "design";
const DESIGN_WINDOW_TITLE: &str = "Design · PADE";
/// The design window's opening and smallest sizes, in logical pixels.
const DESIGN_WINDOW_SIZE: (f64, f64) = (1100.0, 820.0);
const DESIGN_WINDOW_MINIMUM_SIZE: (f64, f64) = (480.0, 480.0);

/// Open a registry-selected design tool. The renderer supplies only its opaque id,
/// so it cannot navigate the application-branded webview to an arbitrary origin.
#[tauri::command]
pub async fn design_open(app: AppHandle, id: String) -> Result<(), String> {
    let definition = REGISTRY
        .iter()
        .find(|definition| definition.id == id)
        .ok_or("unknown design tool")?;
    let target = Url::parse(definition.url).map_err(|error| error.to_string())?;
    if let Some(window) = app.get_webview_window(DESIGN_WINDOW) {
        window.navigate(target).map_err(|error| error.to_string())?;
        window.show().map_err(|error| error.to_string())?;
        return window.set_focus().map_err(|error| error.to_string());
    }
    let (width, height) = DESIGN_WINDOW_SIZE;
    let (minimum_width, minimum_height) = DESIGN_WINDOW_MINIMUM_SIZE;
    WebviewWindowBuilder::new(&app, DESIGN_WINDOW, WebviewUrl::External(target))
        .title(DESIGN_WINDOW_TITLE)
        .inner_size(width, height)
        .min_inner_size(minimum_width, minimum_height)
        .build()
        .map(|_| ())
        .map_err(|error| error.to_string())
}
