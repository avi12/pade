// The steps of a session's life in a window - launch, close, self-exit, in-place
// restart, graceful leave, adoption and hand-off - each acting on the one
// `WindowLayout` and reporting to the shell through its host.

import { Command, sendMessage } from "@/lib/ipc/messaging";
import { didFailToStart } from "@/lib/session/lifecycle/session-exit";
import { rekeyLayout } from "@/lib/session/lifecycle/session-restart";
import { rekeySession } from "@/lib/session/lifecycle/session-scope";
import { newSession } from "@/lib/shell/session-launch";
import type { LaunchOptions } from "@/lib/shell/session-launch";
import type { WindowLayout } from "@/lib/shell/windowLayout.svelte";
import { setPendingPrompt } from "@/lib/stores/session/pendingPrompts.svelte";
import { setNaming } from "@/lib/stores/session/sessionNaming.svelte";
import { isSessionIdle, whenSessionIdle } from "@/lib/stores/session/sessions.svelte";
import { showToast } from "@/lib/stores/ui/toast.svelte";
import type { Agent, AgentSession } from "@/lib/types/agent";

// How long a deliberate leave waits for a busy agent to reach an idle prompt
// before killing it anyway - graceful, but never a trap behind a wedged agent.
const GRACEFUL_LEAVE_TIMEOUT_MS = 30_000;

/** What the shell lends its sessions. */
export interface WindowSessionsHost {
  /** Spring a pane that just joined a split in. */
  animateIn: (id: string) => void;
  /** A session was launched - the workspace is on screen. */
  onLaunched: () => void;
  /** A session id left for good - closed by hand, quit on its own, replaced
   *  by a handoff successor or re-keyed by a restart - rather than left with
   *  the window or moved to another one. */
  onRetired: (session: AgentSession) => void;
  /** The window's last session just ended; the shell decides what next. */
  onEmptied: (ended: { agent: Agent; project: string; isFailedStart: boolean }) => void;
}

export interface LifecycleContext {
  layout: WindowLayout;
  host: WindowSessionsHost;
  namesWithAi: (project: string) => boolean;
  seedNaming: (session: AgentSession) => void;
}

export function launchSession({ context, options }: { context: LifecycleContext; options: LaunchOptions }) {
  const { layout, host, namesWithAi } = context;
  const session = newSession({
    options,
    project: options.project ?? layout.activeProject
  });
  const iReplaced = layout.sessions.findIndex(candidate => candidate.id === options.replacing);
  const isReplacing = iReplaced !== -1;
  if (isReplacing) {
    layout.sessions.splice(iReplaced + 1, 0, session);
  } else {
    layout.sessions.push(session);
  }

  setPendingPrompt({
    id: session.id,
    prompt: options.initialPrompt
  });
  // A new tab is born under its project's setting - the same one the effect
  // in `windowSessionNaming` applies to the tabs already open.
  setNaming({
    id: session.id,
    agent: session.agent.command,
    isOn: namesWithAi(session.project)
  });
  layout.sessionLaunchedAt.set(session.id, Date.now());

  if (isReplacing) {
    layout.paneIds = layout.paneIds.map(paneId => (paneId === options.replacing ? session.id : paneId));

    if (layout.activeId === options.replacing) {
      layout.activeId = session.id;
    }

    return session.id;
  }

  layout.activeId = session.id;
  layout.paneIds = options.split ? [...layout.paneIds, session.id] : [session.id];

  if (options.split) {
    host.animateIn(session.id);
  }

  host.onLaunched();
  return session.id;
}

// Kill these sessions' PTYs and detach them, claiming each exit event as a
// hand-close so the exit handler never respawns or re-routes over it.
export async function closeSessions({ layout, ids }: { layout: WindowLayout; ids: readonly string[] }) {
  for (const id of ids) {
    layout.closingByHand.add(id);
  }

  await Promise.all(ids.map(id => sendMessage(Command.ptyKill, { id })));

  for (const id of ids) {
    layout.detach(id);
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
export async function closeAllGracefully(layout: WindowLayout) {
  const hasBusySession = layout.sessions.some(session => !isSessionIdle(session.id));
  if (hasBusySession) {
    showToast("Waiting for the agent to finish before leaving…");
  }

  await Promise.all(
    layout.sessions.map(session =>
      whenSessionIdle({
        id: session.id,
        timeoutMs: GRACEFUL_LEAVE_TIMEOUT_MS
      })
    )
  );
  await closeSessions({
    layout,
    ids: layout.sessions.map(session => session.id)
  });
}

export async function closeSession({ context, session }: { context: LifecycleContext; session: AgentSession }) {
  const { layout, host } = context;
  layout.closingByHand.add(session.id);
  await sendMessage(Command.ptyKill, { id: session.id });

  // `pty_kill` yields to the event loop. During that gap a project switch (or
  // another close) can already have detached this session and launched a
  // replacement. A stale close must never send that live workspace back to
  // onboarding.
  if (!layout.isOpen(session.id)) {
    layout.closingByHand.delete(session.id);
    return;
  }

  layout.detach(session.id);
  host.onRetired(session);

  // A hand-close is never a failed start: a real project replaces the
  // window's final tab with a fresh one rather than dropping the user into
  // onboarding. A project's last tab closing while other projects' sessions
  // stay open just hands the front to one of them.
  if (layout.sessions.length === 0) {
    host.onEmptied({
      agent: session.agent,
      project: session.project,
      isFailedStart: false
    });
  }
}

// A self-exit right after launch is a failed start, never respawned into a loop.
function isFailedStart({ layout, id }: { layout: WindowLayout; id: string }) {
  return didFailToStart({
    launchedAt: layout.sessionLaunchedAt.get(id) ?? 0,
    now: Date.now()
  });
}

// The PTY exited on its own - e.g. the user pressed Ctrl-C to quit the agent.
export async function sessionExited({ context, id }: { context: LifecycleContext; id: string }) {
  const { layout, host } = context;
  const session = layout.sessions.find(candidate => candidate.id === id);
  if (layout.closingByHand.has(id) || !session) {
    return;
  }

  const wasLastSession = layout.sessions.length === 1;
  const isExitFailedStart = isFailedStart({
    layout,
    id
  });
  layout.detach(id);
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
export async function restartSessions({
  context,
  targets
}: {
  context: LifecycleContext;
  targets: readonly AgentSession[];
}) {
  const { layout, host } = context;
  await Promise.all(
    targets.map(target => {
      layout.closingByHand.add(target.id);
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
    layout.sessionLaunchedAt.delete(target.id);
    layout.closingByHand.delete(target.id);
    layout.sessionLaunchedAt.set(restartedId, Date.now());
    host.onRetired(target);
  }

  layout.assign(
    rekeyLayout({
      ...layout.layout,
      rekeyed
    })
  );
}

/** Take in sessions another window handed over, their PTYs already moved
 *  here and still running: each terminal re-attaches through its usual
 *  path (spawn is a no-op for a live session, `pty_history` replays it). */
export function adoptSessions({
  context,
  adopted,
  activeId
}: {
  context: LifecycleContext;
  adopted: readonly AgentSession[];
  activeId: string | null;
}) {
  const { layout, host, seedNaming } = context;
  const newcomers = adopted.filter(session => !layout.isOpen(session.id));
  layout.sessions.push(...newcomers);
  for (const session of newcomers) {
    layout.sessionLaunchedAt.set(session.id, Date.now());
    seedNaming(session);
  }

  const front = newcomers.find(session => session.id === activeId) ?? newcomers.at(-1);
  if (front !== undefined) {
    layout.select(front.id);
    host.onLaunched();
  }
}

/** Drop one session handed off to a successor, leaving the active pane alone. */
export function removeHandedOff({ context, id }: { context: LifecycleContext; id: string }) {
  const { layout, host } = context;
  const handedOff = layout.sessions.find(session => session.id === id);
  if (handedOff !== undefined) {
    host.onRetired(handedOff);
  }

  layout.sessions = layout.sessions.filter(session => session.id !== id);
  layout.paneIds = layout.paneIds.filter(paneId => paneId !== id);
  layout.forget(id);
}
