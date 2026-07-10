//! AI design / UI-generation tools — quick-launch from PADE.
//!
//! A parallel to the IDE menu: PADE is agent-first, but sometimes you want to
//! sketch or generate a UI in a design-to-code tool — Claude, Google Stitch,
//! Vercel v0, and peers. This exposes a curated roster the topbar opens in the
//! default browser. Adding one is a single `REGISTRY` entry (DRY); nothing else
//! hard-codes a product.

use serde::Serialize;

struct DesignDef {
    id: &'static str,
    label: &'static str,
    /// Vendor, shown as a subtle tag next to the name.
    vendor: &'static str,
    /// Where the tool lives (opened in the default browser).
    url: &'static str,
}

/// Known AI design/UI-generation products, Claude first.
const REGISTRY: &[DesignDef] = &[
    DesignDef {
        id: "claude",
        label: "Claude",
        vendor: "Anthropic",
        url: "https://claude.ai/new",
    },
    DesignDef {
        id: "stitch",
        label: "Stitch",
        vendor: "Google",
        url: "https://stitch.withgoogle.com",
    },
    DesignDef {
        id: "v0",
        label: "v0",
        vendor: "Vercel",
        url: "https://v0.app",
    },
    DesignDef {
        id: "figma-make",
        label: "Figma Make",
        vendor: "Figma",
        url: "https://www.figma.com/make",
    },
    DesignDef {
        id: "lovable",
        label: "Lovable",
        vendor: "Lovable",
        url: "https://lovable.dev",
    },
    DesignDef {
        id: "bolt",
        label: "Bolt",
        vendor: "StackBlitz",
        url: "https://bolt.new",
    },
];

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DesignTool {
    id: String,
    label: String,
    vendor: String,
    url: String,
}

/// The curated roster of AI design/UI-generation tools.
#[tauri::command]
pub fn design_tools() -> Vec<DesignTool> {
    REGISTRY
        .iter()
        .map(|d| DesignTool {
            id: d.id.into(),
            label: d.label.into(),
            vendor: d.vendor.into(),
            url: d.url.into(),
        })
        .collect()
}
