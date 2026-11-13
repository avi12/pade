import { Command, sendMessage } from "@/lib/ipc/messaging";
import { chatsForProject } from "@/lib/session/chats/chat-registry";
import { clearSessionSnapshot, restoreLiveSnapshot } from "@/lib/session/lifecycle/session-restore";
import { effective } from "@/lib/settings/prefs.svelte";
import { sendSettingsMessage } from "@/lib/settings/settings-requests";
import type { createActiveRepository } from "@/lib/shell/activeRepository.svelte";
import type { AgentDetection } from "@/lib/shell/app/agentDetection.svelte";
import type { ChatRegistry } from "@/lib/shell/app/chatRegistry.svelte";
import type { ProjectFlow } from "@/lib/shell/app/projectFlow.svelte";
import { Phase } from "@/lib/shell/app/screenPhase.svelte";
import type { ScreenPhase } from "@/lib/shell/app/screenPhase.svelte";
import { WindowRoute, windowRouteFor } from "@/lib/shell/window/window-route";
import type { WindowSessions } from "@/lib/shell/windowSessions.svelte";
import { initNewProjectDetection } from "@/lib/stores/workspace/active-project/new-project-detection";
import { StartMode } from "@/lib/types/settings";

type BootRoute = Exclude<ReturnType<typeof windowRouteFor>, { kind: typeof WindowRoute.LAUNCH_CONTEXT }>;

export interface BootDependencies {
  windowSessions: WindowSessions;
  repository: ReturnType<typeof createActiveRepository>;
  screen: ScreenPhase;
  registry: ChatRegistry;
  detection: AgentDetection;
  projectFlow: ProjectFlow;
  takeHandoffs: () => Promise<boolean>;
}

export async function boot(dependencies: BootDependencies) {
  const { detection } = dependencies;
  await initNewProjectDetection();
  await Promise.all([detection.detectWithinBootBudget(), sendSettingsMessage(Command.SETTINGS_GET)]);

  // An accidental reload (F5, a crash recovery) re-attaches this window to
  // the agents still running in the backend - before any query routing,
  // whose `w=` went stale the moment the user moved on inside the window
  // (the incident: a `?w=empty` window living in a project rebooted to the
  // picker and orphaned its live PTYs, unreachable).
  if (await reattachAfterReload(dependencies)) {
    return;
  }

  // A spawned window carries a `w=` query that overrides the normal cold-start.
  // The plain main window (no query) keeps today's launch_context behavior.
  if (await routeFromAddress(dependencies)) {
    return;
  }

  await bootFromLaunchContext(dependencies);
}

// Boot a spawned window from its `w=` address (lib/shell/window/window-route). Returns
// true when it handled the launch, so the launch_context path is skipped.
async function routeFromAddress(dependencies: BootDependencies) {
  const route = windowRouteFor(location.search);
  if (route.kind === WindowRoute.LAUNCH_CONTEXT) {
    return false;
  }

  await bootRoute({
    dependencies,
    route
  });
  return true;
}

async function bootRoute({ dependencies, route }: { dependencies: BootDependencies; route: BootRoute }) {
  const { projectFlow, screen } = dependencies;
  if (route.kind === WindowRoute.TEMP) {
    await projectFlow.startInTempWorkspace();
  } else if (route.kind === WindowRoute.PICKER) {
    screen.setPhase(Phase.PROJECT);
  } else if (route.kind === WindowRoute.HANDOFF) {
    await bootFromHandoff({
      dependencies,
      path: route.path
    });
  } else {
    await projectFlow.openProject({ path: route.path });
  }
}

// A window spawned to receive sessions another window handed it: take them
// in, or - when nothing arrived after all - just open the project.
async function bootFromHandoff({ dependencies, path }: { dependencies: BootDependencies; path: string }) {
  const { windowSessions, takeHandoffs, projectFlow, screen, repository } = dependencies;
  windowSessions.enter(path);
  const hasTakenHandoff = await takeHandoffs();
  if (!hasTakenHandoff) {
    await projectFlow.openProject({ path });
    return;
  }

  await screen.showWorkspace();
  await repository.load();
}

async function bootFromLaunchContext(dependencies: BootDependencies) {
  const { projectFlow, repository, screen } = dependencies;
  const context = await sendMessage(Command.LAUNCH_CONTEXT);
  if (context.hasProject) {
    await sendMessage(Command.WORKSPACE_OPEN, { path: context.cwd }); // records it in recent history
    await projectFlow.startAgentFlow({ path: context.cwd });
    await repository.load();
    return;
  }

  // Default: show the homescreen (the project picker). Starting straight
  // in a temp workspace, resuming the last project on the way, is the
  // opt-in the picker's "On launch" section offers instead.
  const isPickerPreferred = effective.startMode === StartMode.enum.picker;
  if (isPickerPreferred) {
    screen.setPhase(Phase.PROJECT);
    return;
  }

  if (await restoreLastProject(dependencies)) {
    return;
  }

  // Default: start immediately in a throwaway workspace so there's no
  // blocking picker. The user can switch any time (Switch button).
  await projectFlow.startInTempWorkspace();
}

// Continue where the last session left off: reopen the project the user
// last worked in, with its chats restored, when it still has any and is
// still on disk. False when there is nothing to continue, or it is gone.
async function restoreLastProject({ registry, projectFlow, repository }: BootDependencies) {
  const lastProject = registry.registry.lastProject;
  const hasLastChats =
    lastProject !== "" &&
    chatsForProject({
      registry: registry.registry,
      project: lastProject
    }).length > 0;
  if (!hasLastChats) {
    return false;
  }

  try {
    await sendMessage(Command.WORKSPACE_OPEN, { path: lastProject });
    await projectFlow.startAgentFlow({ path: lastProject });
    await repository.load();
    return true;
  } catch {
    return false;
  }
}

// Re-attach the live sessions an accidental reload orphaned. The snapshot
// (per window label - survives a reload and a crash rebuild) says which panes
// this window was showing; `pty_list` says which of them the backend still
// hosts, and only that intersection is restored - a deliberate leave killed
// its PTYs, so nothing survives it and boot proceeds normally. Each restored
// pane re-attaches through Terminal's existing path: spawn is a no-op for a
// running session and `pty_history` replays the conversation.
async function reattachAfterReload({ windowSessions, screen, repository }: BootDependencies) {
  const snapshot = await restoreLiveSnapshot();
  if (!snapshot) {
    return false;
  }

  try {
    await sendMessage(Command.WORKSPACE_OPEN, { path: snapshot.project });
  } catch {
    // The project vanished while the window was reloading (deleted, moved,
    // unmounted). Boot falls through to the picker - so kill the snapshot's
    // still-live sessions rather than leave them running unreachably, which
    // would recreate the very invisible-agent incident restore exists to fix.
    await Promise.allSettled(snapshot.sessions.map(orphan => sendMessage(Command.PTY_KILL, { id: orphan.id })));
    clearSessionSnapshot();
    return false;
  }

  windowSessions.enter(snapshot.project);
  windowSessions.restore(snapshot);
  await screen.showWorkspace();
  await repository.load();
  return true;
}
