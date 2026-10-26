// This window's agent sessions and how they are laid out: which exist, which
// are shown side by side and which is active - plus their whole lifecycle,
// launch through re-key to teardown. The shell owns one instance; the layout
// math lives in `@/lib/session/panes/pane-layout`, the policy for an emptied
// workspace stays with the shell (`onEmptied`).

import { aiNamingEnabled } from "@/lib/agent/ai-naming-scope";
import { Command, sendMessage } from "@/lib/ipc/messaging";
import { startingModelChoice } from "@/lib/model/agent-model-scope";
import { frontProject, sessionsOfProject } from "@/lib/session/chats/session-projects";
import { didFailToStart } from "@/lib/session/lifecycle/session-exit";
import { rekeyLayout } from "@/lib/session/lifecycle/session-restart";
import { forgetSession, rekeySession } from "@/lib/session/lifecycle/session-scope";
import { layoutWithoutSession, layoutWithoutSessions, wrappedIndex } from "@/lib/session/panes/pane-layout";
import type { PaneLayout } from "@/lib/session/panes/pane-layout";
import { settings } from "@/lib/settings/settings.svelte";
import { createAgentRetheme } from "@/lib/stores/agent/agentRetheme.svelte";
import { applyNamingToAll, isNaming, setNaming } from "@/lib/stores/session/sessionNaming.svelte";
import { isSessionIdle, whenSessionIdle } from "@/lib/stores/session/sessions.svelte";
import { showToast } from "@/lib/stores/ui/toast.svelte";
import { activeMemberIn } from "@/lib/stores/workspace/workspaceMembers.svelte";
import type { Agent, AgentSession, ModelChoice } from "@/lib/types";
import { untrack } from "svelte";
import { SvelteMap, SvelteSet } from "svelte/reactivity";

// How long a deliberate leave waits for a busy agent to reach an idle prompt
// before killing it anyway - graceful, but never a trap behind a wedged agent.
const GRACEFUL_LEAVE_TIMEOUT_MS = 30_000;

interface LaunchOptions {
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
}

/** A fresh session for `project`, launched as `options` say. */
function newSession({ options, project }: {
  options: LaunchOptions;
  project: string;
}) {
  const session: AgentSession = {
    id: options.sessionId ?? crypto.randomUUID(),
    agent: options.agent,
    project,
    initialPrompt: options.initialPrompt,
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

/** What the shell lends its sessions. */
interface WindowSessionsHost {
  /** Spring a pane that just joined a split in. */
  animateIn: (id: string) => void;
  /** A session was launched - the workspace is on screen. */
  onLaunched: () => void;
  /** A session id left for good - closed by hand, quit on its own, replaced
   *  by a handoff successor or re-keyed by a restart - rather than left with
   *  the window or moved to another one. */
  onRetired: (session: AgentSession) => void;
  /** The window's last session just ended; the shell decides what next. */
  onEmptied: (ended: {
    agent: Agent;
    project: string;
    isFailedStart: boolean;
  }) => void;
}

export function createWindowSessions(host: WindowSessionsHost) {
  let sessions = $state<AgentSession[]>([]);
  // Which sessions are shown side by side in the terminal area. One id = the
  // classic single pane; more = a split view. Always a subset of `sessions`.
  let paneIds = $state<string[]>([]);
  let activeId = $state<string | null>(null);
  const activeSession = $derived(sessions.find(session => session.id === activeId));
  // The project the window points at while no session is in front: the one
  // it entered, or the one its last session left.
  let fallbackProject = $state("");
  // The window's one project context: the active session's project. Every
  // project-scoped surface (top bar, Change Feed, repository, tasks, address)
  // reads this, so switching sessions across projects switches them all.
  const activeProject = $derived(
    frontProject({
      activeSession,
      fallbackProject
    })
  );
  // The front project's sessions - what the tab strip shows. Every session
  // stays mounted (its terminal live) whichever project is in front.
  const projectSessions = $derived(
    sessionsOfProject({
      sessions,
      project: activeProject
    })
  );

  // When each session launched, so a self-exit right after reads as a failed
  // start (lib/session/lifecycle/session-exit) and is not respawned into a loop.
  const sessionLaunchedAt = new SvelteMap<string, number>();
  // A hand-close also kills the PTY, firing the exit event; the exit handler skips
  // these so the two teardown paths don't race.
  const closingByHand = new SvelteSet<string>();

  // A pane that was one of several in a live split when its session detached
  // collapses its width so the survivors glide across to fill (out:collapsePane).
  // Every other removal - a sole-pane tab close above all - swaps instantly: with
  // nothing beside it, a collapsing pane would only crush the surviving terminal
  // to near-zero width and let it reflow back out, a needless squeeze. So just
  // genuine split members are recorded here (and dropped again on `outroend`);
  // the rest get a zero-duration outro.
  const collapsingSplitPanes = new SvelteSet<string>();

  // Whether a project names its session tabs with AI - its own choice when it
  // has one, else the global default (lib/agent/ai-naming-scope).
  function namesWithAi(project: string) {
    return aiNamingEnabled({
      prefs: settings.prefs,
      project
    });
  }

  // Name a session taken in from another window as its project says - unless
  // a ✦ toggled on by hand came along with it.
  function seedNaming(session: AgentSession) {
    if (isNaming(session.id)) {
      return;
    }

    setNaming({
      id: session.id,
      agent: session.agent.command,
      isOn: namesWithAi(session.project)
    });
  }

  // The naming settings as one comparable key, so the effect below re-runs only
  // when one of them really changes.
  const namingSettingsKey = $derived(
    JSON.stringify([settings.prefs.aiSessionNaming, settings.prefs.aiSessionNamingProjects])
  );

  // Apply that setting to every tab already open, each under its own project,
  // and again whenever it changes. `sessions` is read untracked on purpose: this
  // effect exists to follow the SETTING, and re-running it on every tab open or
  // close would undo a ✦ the user had just flipped by hand. New tabs are seeded
  // in `launch` instead.
  $effect(() => {
    applyNamingToAll({
      sessions: untrack(() => sessions),
      isOnFor: project => namingSettingsKey !== "" && namesWithAi(project)
    });
  });

  function launch(options: LaunchOptions) {
    const session = newSession({
      options,
      project: options.project ?? activeProject
    });
    sessions.push(session);
    // A new tab is born under its project's setting - the same one the effect
    // above applies to the tabs already open.
    setNaming({
      id: session.id,
      agent: session.agent.command,
      isOn: namesWithAi(session.project)
    });
    sessionLaunchedAt.set(session.id, Date.now());
    activeId = session.id;
    paneIds = options.split ? [...paneIds, session.id] : [session.id];

    if (options.split) {
      host.animateIn(session.id);
    }

    host.onLaunched();
    return session.id;
  }

  // A tab click shows that session as the sole pane (classic single view).
  // Collapsing a split is deliberately instant (as the design is): a View
  // Transition here would scale the surviving pane's snapshot as it grows to
  // fill, ghosting the old half-width terminal text over the reflowed full-width
  // grid - the platform morph can't track a terminal that repaints on resize.
  function select(id: string) {
    activeId = id;
    paneIds = [id];
  }

  // Ctrl+Tab / Alt+Arrow - cycle the active tab among the front project's
  // tabs (the ones the strip shows), wrapping at the ends.
  function step(delta: number) {
    if (projectSessions.length === 0) {
      return;
    }

    const iNext = wrappedIndex({
      index: projectSessions.findIndex(session => session.id === activeId),
      delta,
      count: projectSessions.length
    });
    select(projectSessions[iNext].id);
  }

  // The window keeps pointing at the project its last session leaves.
  function keepProjectOfLastSession(id: string) {
    const isLastSession = sessions.length === 1 && sessions[0].id === id;
    if (isLastSession) {
      fallbackProject = sessions[0].project;
    }
  }

  // The caller kills the PTY and decides the empty-workspace policy.
  function detach(id: string) {
    if (paneIds.length > 1 && paneIds.includes(id)) {
      collapsingSplitPanes.add(id);
    }

    keepProjectOfLastSession(id);

    ({ sessions, paneIds, activeId } = layoutWithoutSession({
      layout: {
        sessions,
        paneIds,
        activeId
      },
      id
    }));
    forget(id);
  }

  // A session left this window - ended, re-keyed or moved away: everything
  // kept for it goes, here and in every per-session store (session-scope).
  function forget(id: string) {
    sessionLaunchedAt.delete(id);
    closingByHand.delete(id);
    forgetSession(id);
  }

  // Kill these sessions' PTYs and detach them, claiming each exit event as a
  // hand-close so the exit handler never respawns or re-routes over it.
  async function closeSessions(ids: readonly string[]) {
    for (const id of ids) {
      closingByHand.add(id);
    }

    await Promise.all(ids.map(id => sendMessage(Command.ptyKill, { id })));

    for (const id of ids) {
      detach(id);
    }
  }

  // A DELIBERATE leave (switching project, going back to the picker) ends every
  // session - but gracefully: wait for each to reach an idle prompt first
  // (sessionStatus ready, the output-quiet signal - never child-process
  // counting, which mis-reads persistent MCP servers) so nothing mid-flight is
  // severed; the agent auto-saves, so /resume covers continuity. The wait runs
  // before any phase change, while the Terminals are still mounted and
  // publishing status. An accidental reload records no leave - its sessions
  // stay alive and the next boot re-attaches them (session-restore). Each PTY
  // is then killed, reaping its child so the old workspace's cwd lock is
  // released.
  async function closeAllGracefully() {
    const hasBusySession = sessions.some(session => !isSessionIdle(session.id));
    if (hasBusySession) {
      showToast("Waiting for the agent to finish before leaving…");
    }

    await Promise.all(
      sessions.map(session => whenSessionIdle({
        id: session.id,
        timeoutMs: GRACEFUL_LEAVE_TIMEOUT_MS
      }))
    );
    await closeSessions(sessions.map(session => session.id));
  }

  async function close(session: AgentSession) {
    closingByHand.add(session.id);
    await sendMessage(Command.ptyKill, { id: session.id });

    // `pty_kill` yields to the event loop. During that gap a project switch (or
    // another close) can already have detached this session and launched a
    // replacement. A stale close must never send that live workspace back to
    // onboarding.
    const isStillOpen = sessions.some(current => current.id === session.id);
    if (!isStillOpen) {
      closingByHand.delete(session.id);
      return;
    }

    detach(session.id);
    host.onRetired(session);

    // A hand-close is never a failed start: a real project replaces the
    // window's final tab with a fresh one rather than dropping the user into
    // onboarding. A project's last tab closing while other projects' sessions
    // stay open just hands the front to one of them.
    if (sessions.length === 0) {
      host.onEmptied({
        agent: session.agent,
        project: session.project,
        isFailedStart: false
      });
    }
  }

  // A self-exit right after launch is a failed start, never respawned into a loop.
  function isFailedStart(id: string) {
    return didFailToStart({
      launchedAt: sessionLaunchedAt.get(id) ?? 0,
      now: Date.now()
    });
  }

  // The PTY exited on its own - e.g. the user pressed Ctrl-C to quit the agent.
  async function exited(id: string) {
    const session = sessions.find(candidate => candidate.id === id);
    if (closingByHand.has(id) || !session) {
      return;
    }

    const wasLastSession = sessions.length === 1;
    const isExitFailedStart = isFailedStart(id);
    detach(id);
    // Reap the backend record; the child is already gone, so a failure is moot.
    await Promise.allSettled([sendMessage(Command.ptyKill, { id })]);
    host.onRetired(session);

    if (wasLastSession) {
      host.onEmptied({
        agent: session.agent,
        project: session.project,
        isFailedStart: isExitFailedStart
      });
    }
  }

  // Kill the targets and re-key them to fresh ids in place - each terminal
  // remounts, respawns its agent (resuming its conversation via the session id),
  // and the pane layout follows. The shared core behind every in-place restart
  // (spawn-time theme changes, a model switch).
  async function restart(targets: readonly AgentSession[]) {
    await Promise.all(
      targets.map(target => {
        closingByHand.add(target.id);
        return sendMessage(Command.ptyKill, { id: target.id });
      })
    );

    const rekeyed: Record<string, string> = {};
    for (const target of targets) {
      const restartedId = crypto.randomUUID();
      rekeyed[target.id] = restartedId;
      // What outlives a restart - the tab's name, its ✦ auto-naming - moves to
      // the new id; everything else kept for the old one goes.
      rekeySession({
        fromId: target.id,
        toId: restartedId
      });
      sessionLaunchedAt.delete(target.id);
      closingByHand.delete(target.id);
      sessionLaunchedAt.set(restartedId, Date.now());
      host.onRetired(target);
    }

    const relaid = rekeyLayout({
      sessions,
      paneIds,
      activeId,
      rekeyed
    });
    sessions = relaid.sessions;
    paneIds = relaid.paneIds;
    activeId = relaid.activeId;
  }

  // Hand a session the args that resume its own conversation, so the restart
  // that follows reopens it rather than starting a blank one.
  function applyResumeArgs({ id, args }: {
    id: string;
    args: string[];
  }) {
    if (args.length === 0) {
      return;
    }

    sessions = sessions.map(session => session.id === id ? {
      ...session,
      args
    } : session);
  }

  // A terminal-scheme flip reaches the running agents through
  // lib/stores/agent/agentRetheme: Claude repaints in place, the spawn-themed agents
  // are respawned on their own conversation once idle.
  const agentRetheme = createAgentRetheme({
    sessions: () => sessions,
    applyResumeArgs,
    restartSessions: restart
  });
  $effect(() => agentRetheme.recordSpawnSchemes());
  $effect(() => agentRetheme.followSchemeChange());

  return {
    get sessions() {
      return sessions;
    },
    set sessions(next) {
      sessions = next;
    },
    get paneIds() {
      return paneIds;
    },
    set paneIds(next) {
      paneIds = next;
    },
    get activeId() {
      return activeId;
    },
    set activeId(next) {
      activeId = next;
    },
    get activeSession() {
      return activeSession;
    },
    /** The window's project context - the active session's project. */
    get activeProject() {
      return activeProject;
    },
    /** The front project's sessions, as the tab strip shows them. */
    get projectSessions() {
      return projectSessions;
    },
    /** Point the window at `project` while no session of it is in front yet -
     *  entering it, or going back to the picker (empty). */
    enter(project: string) {
      fallbackProject = project;
    },
    collapsingSplitPanes,
    launch,
    select,
    step,
    close,
    exited,
    closeSessions,
    closeAllGracefully,
    restart,
    applyResumeArgs,
    isOpen: (id: string) => sessions.some(session => session.id === id),
    /** Re-attach the sessions a reload orphaned, stamped as launched-now so one
     *  that dies right after reads as a failed start (no respawn loop). */
    restore(layout: PaneLayout) {
      ({ sessions, paneIds, activeId } = layout);
      for (const restored of layout.sessions) {
        sessionLaunchedAt.set(restored.id, Date.now());
      }
    },
    /** Claim these sessions' coming exit events as hand-closes. */
    markClosing(ids: Iterable<string>) {
      for (const id of ids) {
        closingByHand.add(id);
      }
    },
    /** Drop sessions whose PTYs were killed under `markClosing`. */
    removeClosed(ids: ReadonlySet<string>) {
      ({ sessions, paneIds, activeId } = layoutWithoutSessions({
        layout: {
          sessions,
          paneIds,
          activeId
        },
        ids
      }));

      for (const id of ids) {
        forget(id);
      }
    },
    /** Let go of sessions another window now owns (moved there with their PTYs
     *  still running): dropped from this window without a kill, and without
     *  ending them - their chats live on in the receiving window. */
    release(ids: readonly string[]) {
      for (const id of ids) {
        detach(id);
      }
    },
    /** Take in sessions another window handed over, their PTYs already moved
     *  here and still running: each terminal re-attaches through its usual
     *  path (spawn is a no-op for a live session, `pty_history` replays it). */
    adopt({ adopted, activeId: adoptedActiveId }: {
      adopted: readonly AgentSession[];
      activeId: string | null;
    }) {
      const newcomers = adopted.filter(session => !sessions.some(current => current.id === session.id));
      sessions.push(...newcomers);
      for (const session of newcomers) {
        sessionLaunchedAt.set(session.id, Date.now());
        seedNaming(session);
      }

      const front = newcomers.find(session => session.id === adoptedActiveId) ?? newcomers.at(-1);
      if (front !== undefined) {
        select(front.id);
        host.onLaunched();
      }
    },
    /** Drop one session handed off to a successor, leaving the active pane alone. */
    removeHandedOff(id: string) {
      const handedOff = sessions.find(session => session.id === id);
      if (handedOff !== undefined) {
        host.onRetired(handedOff);
      }

      sessions = sessions.filter(session => session.id !== id);
      paneIds = paneIds.filter(paneId => paneId !== id);
      forget(id);
    }
  };
}

export type WindowSessions = ReturnType<typeof createWindowSessions>;
