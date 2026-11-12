// This window's agent sessions and how they are laid out: which exist, which
// are shown side by side and which is active - plus their whole lifecycle,
// launch through re-key to teardown. The shell owns one instance; the layout
// math lives in `@/lib/session/panes/pane-layout`, the policy for an emptied
// workspace stays with the shell (`onEmptied`).

import type { PaneLayout } from "@/lib/session/panes/pane-layout";
import type { LaunchOptions } from "@/lib/shell/session-launch";
import {
  adoptSessions,
  closeAllGracefully,
  closeSession,
  closeSessions,
  launchSession,
  removeHandedOff,
  restartSessions,
  sessionExited
} from "@/lib/shell/window-session-lifecycle";
import type { WindowSessionsHost } from "@/lib/shell/window-session-lifecycle";
import { layoutAccessors, WindowLayout } from "@/lib/shell/windowLayout.svelte";
import { createSessionNaming } from "@/lib/shell/windowSessionNaming.svelte";
import { createAgentRetheme } from "@/lib/stores/agent/agentRetheme.svelte";
import type { AgentSession } from "@/lib/types/agent";

export function createWindowSessions(host: WindowSessionsHost) {
  const layout = new WindowLayout();
  const { namesWithAi, seedNaming } = createSessionNaming({ sessions: () => layout.sessions });
  const context = {
    layout,
    host,
    namesWithAi,
    seedNaming
  };
  function restart(targets: readonly AgentSession[]) {
    return restartSessions({
      context,
      targets
    });
  }

  // A terminal-scheme flip reaches the running agents through
  // lib/stores/agent/agentRetheme: Claude repaints in place, the spawn-themed agents
  // are respawned on their own conversation once idle.
  const agentRetheme = createAgentRetheme({
    sessions: () => layout.sessions,
    applyResumeArgs: args => layout.applyResumeArgs(args),
    restartSessions: restart
  });
  $effect(() => agentRetheme.recordSpawnSchemes());
  $effect(() => agentRetheme.followSchemeChange());

  return Object.assign(layoutAccessors(layout), {
    /** Point the window at `project` while no session of it is in front yet -
     *  entering it, or going back to the picker (empty). */
    enter(project: string) {
      layout.fallbackProject = project;
    },
    launch: (options: LaunchOptions) => launchSession({
      context,
      options
    }),
    select: (id: string) => layout.select(id),
    step: (delta: number) => layout.step(delta),
    close: (session: AgentSession) => closeSession({
      context,
      session
    }),
    exited: (id: string) => sessionExited({
      context,
      id
    }),
    closeSessions: (ids: readonly string[]) => closeSessions({
      layout,
      ids
    }),
    closeAllGracefully: () => closeAllGracefully(layout),
    restart,
    applyResumeArgs: (resume: Parameters<WindowLayout["applyResumeArgs"]>[0]) => layout.applyResumeArgs(resume),
    isOpen: (id: string) => layout.isOpen(id),
    /** Re-attach the sessions a reload orphaned, stamped as launched-now so one
     *  that dies right after reads as a failed start (no respawn loop). */
    restore: (restored: PaneLayout) => layout.restore(restored),
    /** Claim these sessions' coming exit events as hand-closes. */
    markClosing: (ids: Iterable<string>) => layout.markClosing(ids),
    /** Drop sessions whose PTYs were killed under `markClosing`. */
    removeClosed: (ids: ReadonlySet<string>) => layout.removeClosed(ids),
    /** Let go of sessions another window now owns (moved there with their PTYs
     *  still running): dropped from this window without a kill, and without
     *  ending them - their chats live on in the receiving window. */
    release: (ids: readonly string[]) => layout.release(ids),
    /** Take in sessions another window handed over, their PTYs already moved
     *  here and still running. */
    adopt: ({ adopted, activeId }: {
      adopted: readonly AgentSession[];
      activeId: string | null;
    }) => adoptSessions({
      context,
      adopted,
      activeId
    }),
    /** Drop one session handed off to a successor, leaving the active pane alone. */
    removeHandedOff: (id: string) => removeHandedOff({
      context,
      id
    })
  });
}

export type WindowSessions = ReturnType<typeof createWindowSessions>;
