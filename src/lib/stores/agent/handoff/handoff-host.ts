import type { Agent, AgentSession } from "@/lib/types/agent";

/** What the app shell provides. The reads run inside the shell's `$effect`, so
 *  the scan re-runs as the session list / prefs / context stores change. */
export interface HandoffHost {
  sessions: () => AgentSession[];
  /** The agents installed and available to take over - the crossover pool for a
   *  usage failover. The current agent is excluded at selection time. */
  availableAgents: () => Agent[];
  /** Whether the user opted out via prefs.autoHandoff. */
  isOptedOut: () => boolean;
  /** The percent-of-context that triggers the cycle - the resolved
   *  prefs.handoffPct (`effective.handoffPercentage`), read per scan so a Config
   *  change applies without a restart. */
  thresholdPercentage: () => number;
  /** Source text for the handoff-doc slug (workspace label or short dir). */
  slugSource: () => string;
  /** The session's project root - where its handoff doc lands (and is
   *  deleted from once the successor has consumed it). */
  projectDirectory: (session: AgentSession) => string;
  /** End a session's process as a DELIBERATE close.
   *
   *  It must go through the shell, not `pty_kill` from here: the shell treats an
   *  unannounced exit of the last session as the user quitting the agent and
   *  respawns a replacement. A handoff kill that skipped that flag raced its own
   *  exit event - the respawn landed a bare extra tab and stole the active pane
   *  from the successor being seeded right behind it. */
  endSession: (id: string) => Promise<void>;
  /** Drop an ended session from the shell's tab strip and panes. */
  removeSession: (id: string) => void;
  /** Start the successor agent seeded to continue from the handoff doc.
   *  Returns the new session's id so the doc's consumption can be watched.
   *  `predecessorId` is the session being handed off, so per-session UI state the
   *  shell owns (the ✦ auto-naming toggle) can carry over to the successor. */
  launchSuccessor: (options: {
    agent: Agent;
    cwd?: string;
    initialPrompt: string;
    predecessorId: string;
  }) => string;
}
