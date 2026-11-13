import type { IconName } from "@/lib/icon-name";

// Mirrors the agent ids in src-tauri/src/agents.rs - the closed set the icon map
// keys off, so no bare id string literals leak out.
export const AgentId = {
  CLAUDE: "claude",
  CODEX: "codex",
  COPILOT: "copilot",
  GROK: "grok",
  ANTIGRAVITY: "antigravity",
  CURSOR: "cursor",
  OPENCODE: "opencode",
  AIDER: "aider"
} as const;
type AgentId = (typeof AgentId)[keyof typeof AgentId];

// Each agent's own brand mark. Unlike the stroke-based UI icons, these are filled
// glyphs - the SVGs carry their own `fill="currentColor" stroke="none"`. Aider has
// no distinct brand icon, so it isn't mapped and falls through to the terminal mark.
const AGENT_ICONS: Record<string, IconName> = {
  [AgentId.CLAUDE]: "claude",
  [AgentId.CODEX]: "codex",
  [AgentId.COPILOT]: "copilot",
  [AgentId.GROK]: "grok",
  [AgentId.ANTIGRAVITY]: "antigravity",
  [AgentId.CURSOR]: "cursor",
  [AgentId.OPENCODE]: "opencode"
};
const FALLBACK_ICON: IconName = "terminal";

/** A known agent's own glyph, else the generic terminal mark. */
export function agentIconName(agentId: string) {
  return AGENT_ICONS[agentId] ?? FALLBACK_ICON;
}
