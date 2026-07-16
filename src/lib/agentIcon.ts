import type { IconName } from "@/lib/Icon.svelte";

// Mirrors the agent ids in src-tauri/src/agents.rs — the closed set the icon map
// keys off, so no bare id string literals leak out.
export const AgentId = {
  Claude: "claude",
  Codex: "codex",
  Grok: "grok",
  Antigravity: "antigravity",
  Cursor: "cursor",
  Aider: "aider"
} as const;
export type AgentId = (typeof AgentId)[keyof typeof AgentId];

// Each agent's own brand mark. Unlike the stroke-based UI icons, these are filled
// glyphs — the SVGs carry their own `fill="currentColor" stroke="none"`. Aider has
// no distinct brand icon, so it isn't mapped and falls through to the terminal mark.
const AGENT_ICONS: Record<string, IconName> = {
  [AgentId.Claude]: "claude",
  [AgentId.Codex]: "codex",
  [AgentId.Grok]: "grok",
  [AgentId.Antigravity]: "antigravity",
  [AgentId.Cursor]: "cursor"
};
const FALLBACK_ICON: IconName = "terminal";

/** A known agent's own glyph, else the generic terminal mark. */
export function agentIconName(agentId: string): IconName {
  return AGENT_ICONS[agentId] ?? FALLBACK_ICON;
}
