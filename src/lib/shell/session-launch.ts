import { startingModelChoice } from "@/lib/model/agent-model-scope";
import { settings } from "@/lib/settings/settings.svelte";
import { activeMemberIn } from "@/lib/stores/workspace/active-project/workspaceMembers.svelte";
import type { Agent, AgentSession, ModelChoice } from "@/lib/types/agent";

export interface LaunchOptions {
  agent: Agent;
  /** The project the session belongs to - the active project when absent. */
  project?: string;
  /** Reuse this session id instead of minting one - restoring a persisted
   *  chat keeps its identity (and its snapshot entry) across a relaunch. */
  sessionId?: string;
  initialPrompt?: string;
  cwd?: string;
  branch?: string;
  /** Extra command args - the project path when running a terminal editor. */
  args?: string[];
  /** Add alongside the current panes (split) instead of replacing them. */
  split?: boolean;
  /** A model / effort to run this session on (a successor keeping its
   *  predecessor's pick). */
  modelChoice?: ModelChoice;
  /** The conversation to carry on - a session resumed after its folder
   *  moved; a new one otherwise. */
  conversationId?: string;
  /** The session this one takes over from - a handoff successor. It steps
   *  into that tab's place, pane and focus as they are, so a handoff of a tab
   *  in the background stays in the background. */
  replacing?: string;
}

/** A fresh session for `project`, launched as `options` say. */
export function newSession({ options, project }: {
  options: LaunchOptions;
  project: string;
}) {
  const session: AgentSession = {
    id: options.sessionId ?? crypto.randomUUID(),
    agent: options.agent,
    project,
    modelChoice: startingModelChoice({
      explicit: options.modelChoice,
      prefs: settings.prefs,
      project,
      agent: options.agent.id
    }),
    // Never leave an agent to inherit the backend process cwd: another PADE
    // window may switch that shared process to a different project while this
    // terminal is mounting. Worktrees still override the session's project, and
    // a multi-part workspace starts its agents in the member the top bar points
    // at - the root until you pick another (see stores/workspaceMembers).
    cwd: options.cwd ?? activeMemberIn(project),
    branch: options.branch,
    args: options.args,
    // A stable id for this conversation, distinct from the session `id` (which
    // is re-keyed to remount the terminal on a restart). Pins the agent's own
    // conversation so a restart resumes THIS one.
    conversationId: options.conversationId ?? crypto.randomUUID()
  };
  return session;
}
