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

const AGENT_ICONS: Record<string, IconName> = {
  [AgentId.Claude]: "sparkles",
  [AgentId.Codex]: "code",
  [AgentId.Grok]: "activity",
  [AgentId.Antigravity]: "star",
  [AgentId.Cursor]: "pencil",
  [AgentId.Aider]: "git"
};
const FALLBACK_ICON: IconName = "terminal";

/** A known agent's own glyph, else the generic terminal mark. */
export function agentIconName(agentId: string): IconName {
  return AGENT_ICONS[agentId] ?? FALLBACK_ICON;
}
