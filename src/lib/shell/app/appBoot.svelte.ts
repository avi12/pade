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
import { showToast } from "@/lib/stores/ui/toast.svelte";
import {
  disposeNewProjectDetection,
  initNewProjectDetection
} from "@/lib/stores/workspace/active-project/new-project-detection";
import { StartMode } from "@/lib/types/settings";
import { onDestroy, onMount } from "svelte";

type BootRoute = Exclude<ReturnType<typeof windowRouteFor>, { kind: typeof WindowRoute.LaunchContext }>;

/** The window's cold start: where it lands first - re-attached sessions, its
 *  `w=` route, the launch context, the last project, or a throwaway workspace.
 *  Call during component initialisation; it boots on mount. */
export function startAppBoot({
  windowSessions,
  repository,
  screen,
  registry,
  detection,
  projectFlow,
  takeHandoffs
}: {
  windowSessions: WindowSessions;
  repository: ReturnType<typeof createActiveRepository>;
  screen: ScreenPhase;
  registry: ChatRegistry;
  detection: AgentDetection;
  projectFlow: ProjectFlow;
  takeHandoffs: () => Promise<boolean>;
}) {
  onMount(async () => {
    try {
      await boot();
    } catch {
      // Never strand the user on the splash - fall back to the project picker.
      showToast("Startup hit a snag - pick a project to continue");
      screen.setPhase(Phase.Project);
    }
  });
  onDestroy(disposeNewProjectDetection);

  async function boot() {
    await initNewProjectDetection();
    await Promise.all([detection.detectWithinBootBudget(), sendSettingsMessage(Command.settingsGet)]);

    // An accidental reload (F5, a crash recovery) re-attaches this window to
    // the agents still running in the backend - before any query routing,
    // whose `w=` went stale the moment the user moved on inside the window
    // (the incident: a `?w=empty` window living in a project rebooted to the
    // picker and orphaned its live PTYs, unreachable).
    if (await reattachAfterReload()) {
      return;
    }

    // A spawned window carries a `w=` query that overrides the normal cold-start.
    // The plain main window (no query) keeps today's launch_context behavior.
    if (await routeFromAddress()) {
      return;
    }

    await bootFromLaunchContext();
  }

  // Boot a spawned window from its `w=` address (lib/shell/window/window-route). Returns
  // true when it handled the launch, so the launch_context path is skipped.
  async function routeFromAddress() {
    const route = windowRouteFor(location.search);
    if (route.kind === WindowRoute.LaunchContext) {
      return false;
    }

    await bootRoute(route);
    return true;
  }

  async function bootRoute(route: BootRoute) {
    if (route.kind === WindowRoute.Temp) {
      await projectFlow.startInTempWorkspace();
    } else if (route.kind === WindowRoute.Picker) {
      screen.setPhase(Phase.Project);
    } else if (route.kind === WindowRoute.Handoff) {
      await bootFromHandoff(route.path);
    } else {
      await projectFlow.openProject({ path: route.path });
    }
  }

  // A window spawned to receive sessions another window handed it: take them
  // in, or - when nothing arrived after all - just open the project.
  async function bootFromHandoff(path: string) {
    windowSessions.enter(path);
    const hasTakenHandoff = await takeHandoffs();
    if (!hasTakenHandoff) {
      await projectFlow.openProject({ path });
      return;
    }

    await screen.showWorkspace();
    await repository.load();
  }

  async function bootFromLaunchContext() {
    const context = await sendMessage(Command.launchContext);
    if (context.hasProject) {
      await sendMessage(Command.workspaceOpen, { path: context.cwd }); // records it in recent history
      await projectFlow.startAgentFlow({ path: context.cwd });
      await repository.load();
      return;
    }

    // Default: show the homescreen (the project picker). Starting straight
    // in a temp workspace, resuming the last project on the way, is the
    // opt-in the picker's "On launch" section offers instead.
    const prefersPicker = effective.startMode === StartMode.enum.picker;
    if (prefersPicker) {
      screen.setPhase(Phase.Project);
      return;
    }

    if (await restoreLastProject()) {
      return;
    }

    // Default: start immediately in a throwaway workspace so there's no
    // blocking picker. The user can switch any time (Switch button).
    await projectFlow.startInTempWorkspace();
  }

  // Continue where the last session left off: reopen the project the user
  // last worked in, with its chats restored, when it still has any and is
  // still on disk. False when there is nothing to continue, or it is gone.
  async function restoreLastProject() {
    const lastProject = registry.registry.lastProject;
    const hasLastChats = lastProject !== "" && chatsForProject({
      registry: registry.registry,
      project: lastProject
    }).length > 0;
    if (!hasLastChats) {
      return false;
    }

    try {
      await sendMessage(Command.workspaceOpen, { path: lastProject });
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
  async function reattachAfterReload() {
    const snapshot = await restoreLiveSnapshot();
    if (!snapshot) {
      return false;
    }

    try {
      await sendMessage(Command.workspaceOpen, { path: snapshot.project });
    } catch {
      // The project vanished while the window was reloading (deleted, moved,
      // unmounted). Boot falls through to the picker - so kill the snapshot's
      // still-live sessions rather than leave them running unreachably, which
      // would recreate the very invisible-agent incident restore exists to fix.
      await Promise.allSettled(snapshot.sessions.map(orphan => sendMessage(Command.ptyKill, { id: orphan.id })));
      clearSessionSnapshot();
      return false;
    }

    windowSessions.enter(snapshot.project);
    windowSessions.restore(snapshot);
    await screen.showWorkspace();
    await repository.load();
    return true;
  }
}
