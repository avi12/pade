// Pure logic for restarting agent sessions when a project's MCP servers change.
//
// The reactive side (killing PTYs, re-keying the live pane state, toasting) lives
// in App.svelte, which owns that state; the error-prone decisions — WHICH
// sessions a config change affects, and HOW the pane layout re-keys — are pure
// functions here so they can be reasoned about and unit-tested on their own.

import { normalizePath, parentDir } from "@/lib/paths";
import type { AgentSession, McpChange } from "@/lib/types";

/** The sessions an MCP-config change should restart: those running an agent the
 *  config governs, whose working directory IS the directory that changed (a
 *  per-branch worktree keeps its own config, so a root change leaves it alone),
 *  and that carry a conversation id to resume back into. */
export function mcpRestartTargets({ sessions, change, currentProject }: {
  sessions: readonly AgentSession[];
  change: McpChange;
  currentProject: string;
}): AgentSession[] {
  const changedRoot = normalizePath(parentDir(change.path) ?? change.path);
  return sessions.filter(session =>
    change.agents.includes(session.agent.command)
    && session.conversationId !== undefined
    && normalizePath(session.cwd ?? currentProject) === changedRoot);
}

/** Apply a re-key (old session id → new id) across the pane layout: each
 *  restarted session gets its new id and drops its `initialPrompt` (already
 *  sent, so the agent resumes rather than re-sends), while the pane order and
 *  active pane follow their sessions to the new ids. Pure — the caller assigns
 *  the results to its reactive state and does the side effects (kill, bookkeeping). */
export function rekeyLayout({ sessions, paneIds, activeId, rekeyed }: {
  sessions: readonly AgentSession[];
  paneIds: readonly string[];
  activeId: string | null;
  rekeyed: Readonly<Record<string, string>>;
}): {
  sessions: AgentSession[];
  paneIds: string[];
  activeId: string | null;
} {
  return {
    sessions: sessions.map(session => {
      const restartedId = rekeyed[session.id];
      return restartedId ? {
        ...session,
        id: restartedId,
        initialPrompt: undefined
      } : session;
    }),
    paneIds: paneIds.map(id => rekeyed[id] ?? id),
    activeId: activeId === null ? null : (rekeyed[activeId] ?? activeId)
  };
}
