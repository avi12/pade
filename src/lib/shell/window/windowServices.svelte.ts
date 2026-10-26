// The background services a window runs for as long as it lives, each keyed on
// the front project where it matters: the backend streams it subscribes to, the
// watch of every project it holds, the branch pill's refresh, temp workspace
// auto-naming, the send-from-IDE shortcut and Discord presence. Each service
// owns its store; this only starts them and names where the front project comes
// from (`setFrontProjectSource`) - the project-scoped stores follow it on their
// own.

import { createAutoNamer } from "@/lib/agent/auto-name";
import { Command, EventType, onMessage, sendMessage } from "@/lib/ipc/messaging";
import { effective } from "@/lib/settings/prefs.svelte";
import { sendSettingsMessage } from "@/lib/settings/settings-requests";
import { settings } from "@/lib/settings/settings.svelte";
import type { createActiveRepository } from "@/lib/shell/activeRepository.svelte";
import { updateDiscordPresence } from "@/lib/shell/window/discord-presence";
import type { WindowSessions } from "@/lib/shell/windowSessions.svelte";
import { ensureChoiceAttention } from "@/lib/stores/session/sessionAttention.svelte";
import { ensureRunnerListeners } from "@/lib/stores/tasks/runners.svelte";
import { taskCatalog } from "@/lib/stores/tasks/taskCatalog.svelte";
import { adoptRunningTasks, initTaskRunDetection } from "@/lib/stores/tasks/taskRuns.svelte";
import { watchAppUpdate } from "@/lib/stores/update/appUpdate.svelte";
import { refreshEditors } from "@/lib/stores/workspace/editors.svelte";
import { keepFeedsOf } from "@/lib/stores/workspace/feed.svelte";
import { frontProject, setFrontProjectSource } from "@/lib/stores/workspace/frontProject.svelte";
import { heldProjects, watchHeldProjects } from "@/lib/stores/workspace/project-watch";
import { projectKind, requestProjectKind } from "@/lib/stores/workspace/projectKinds.svelte";
import { onFrontProjectLanguagesChange } from "@/lib/stores/workspace/projectLanguages.svelte";
import { activeMemberIn, refreshMemberBranches } from "@/lib/stores/workspace/workspaceMembers.svelte";
import { registerSendShortcut, unregisterSendShortcut } from "@/lib/terminal/input/send-shortcut";
import { coalesced } from "@/lib/util/async/coalesce";
import { onMount, untrack } from "svelte";

export function startWindowServices({ windowSessions, repository, isWindowFocused }: {
  windowSessions: WindowSessions;
  repository: ReturnType<typeof createActiveRepository>;
  /** Whether this window has focus - only the focused one publishes presence. */
  isWindowFocused: () => boolean;
}) {
  // The one place the window's front project is named: the active session's
  // project (see windowSessions). Every project-scoped store reads it through
  // `frontProject`.
  setFrontProjectSource(() => windowSessions.activeProject);

  // Subscribe once to the backend task-runner stream so the dock updates live.
  onMount(async () => {
    await ensureRunnerListeners();
  });

  // Arm WebView2 crash auto-recovery with this boot's real URL. The frontend
  // is the one party that always knows it - a backend capture at window
  // creation raced navigation and once rebuilt a crashed window onto
  // about:blank - and a recovery-rebuilt window re-arms itself right here.
  onMount(async () => {
    await sendMessage(Command.recoveryArm, { url: location.href });
  });

  // Watch the PTY stream once for the agent's multiple-choice prompts, so a tab
  // can flash red when one is pending on it (lib/stores/session/sessionAttention).
  onMount(async () => {
    await ensureChoiceAttention();
  });

  // Follow PADE's own self-update, so a ready release is announced and Config
  // can offer the restart into it (lib/stores/update/appUpdate).
  onMount(async () => {
    await watchAppUpdate();
  });

  // Reflect known tasks the agent runs as "running" in the Tasks panel.
  onMount(async () => {
    await initTaskRunDetection();
  });

  // Each time a project's task list arrives - the project came to the front,
  // or the window reloaded onto it - bring back the cards of its tasks still
  // running in the backend.
  $effect(() => {
    adoptRunningTasks(taskCatalog.snapshot);
  });

  // Adopt settings another window - or another PADE instance - changed: it
  // deleted a workspace, created or renamed one, toggled a pin. So this window's
  // rail, switcher and picker never list a project that is already gone or miss
  // one just made. A burst of announcements costs one re-read in flight and at
  // most one more after it. Best-effort because an event handler can't await: a
  // failed re-read leaves the settings already adopted in place.
  const rereadSettings = coalesced(async () => {
    await Promise.allSettled([sendSettingsMessage(Command.settingsGet)]);
  });
  let unlistenSettings: (() => void) | undefined;
  async function subscribeToSettingsChanges() {
    unlistenSettings = await onMessage(EventType.settingsChanged, rereadSettings);
  }
  onMount(() => {
    subscribeToSettingsChanges();
    return () => unlistenSettings?.();
  });

  // Whatever brings a project to the front - opening it, or picking one of its
  // sessions already running here - records it as the most recent, so the
  // rail's order is the order projects were last in front. Best-effort: a
  // project that vanished meanwhile leaves the recents as they were.
  async function recordFrontProject(path: string) {
    await Promise.allSettled([sendMessage(Command.workspaceOpen, { path })]);
    await rereadSettings();
  }
  $effect(() => {
    const path = frontProject();
    if (!path) {
      return;
    }

    recordFrontProject(path);
  });

  // The front project's editors, for the top-bar launcher and the Change Feed's
  // reveal - picked again when its language mix moves.
  onFrontProjectLanguagesChange(refreshEditors);
  $effect(() => {
    const root = frontProject();
    if (root) {
      untrack(() => refreshEditors(root));
    }
  });

  // Watch every project the window holds app-wide - not only while the Change
  // Feed is open - so each gathers its own feed, the Tasks panel auto-updates on
  // a manifest edit, and the task-run and auto-name subscribers see changes from
  // the moment a project opens. A project's feed is kept exactly as long as the
  // window holds it.
  async function watchWindowProjects(roots: string[]) {
    keepFeedsOf(roots);
    try {
      await watchHeldProjects(roots);
    } catch (error) {
      console.warn("pade: could not watch the window's projects", error);
    }
  }
  $effect(() => {
    const roots = heldProjects({
      sessionProjects: windowSessions.sessions.map(session => session.project),
      frontProject: frontProject()
    });
    untrack(() => watchWindowProjects(roots));
  });

  // The branch pill reports the member the switcher points at, so picking a
  // nested checkout re-reads its branch, its branch list and its remote.
  $effect(() => {
    const member = activeMemberIn(frontProject());
    if (!member) {
      return;
    }

    untrack(repository.load);
  });

  // Keep the branch pill honest: a branch switch, git init, or remote change in
  // the front project re-fetches what it shows; a project behind re-reads when
  // it comes to the front.
  let unlistenGitState: (() => void) | undefined;
  async function subscribeToGitState() {
    unlistenGitState = await onMessage(EventType.gitState, async ({ data: { root } }) => {
      if (root !== frontProject()) {
        return;
      }

      await Promise.all([repository.load(), refreshMemberBranches()]);
    });
  }
  onMount(() => {
    subscribeToGitState();
    return () => unlistenGitState?.();
  });

  // ── Discord Rich Presence ────────────────────────────────────────────────────
  // Broadcast "Playing PADE" (opt-in), optionally naming the open project. The
  // state→bridge mapping lives in lib/shell/window/discord-presence (SoC). Both flags resolve
  // through `effective` - the one home for their defaults, shared with the Config
  // panel's toggles - so a change there reaches the presence the moment it lands.
  // $derived so the effect only re-sends when presence, the project-name toggle, or
  // the open project actually change - not on every unrelated settings reassignment
  // (a pin, an editor rule).
  const discordEnabled = $derived(effective.discordPresence);
  const discordShowProject = $derived(effective.discordShowProject);
  // The open project's language, shown VS-Code-style as a small overlay icon +
  // status line. Fetched only while presence will actually show it, and refreshed
  // on a project switch.
  const discordProjectKind = $derived(projectKind(frontProject()) ?? undefined);
  $effect(() => {
    const shown = frontProject();
    if (discordEnabled && discordShowProject && shown) {
      requestProjectKind(shown);
    }
  });
  // Presence follows the FOCUSED window: every PADE window shares one Discord
  // connection (one process), so only the focused window publishes - switching
  // windows switches the reported project, and an unfocused window never
  // overwrites the focused one's status. A blur sends nothing; the window
  // gaining focus overwrites.
  $effect(() => {
    if (!isWindowFocused()) {
      return;
    }

    updateDiscordPresence({
      isEnabled: discordEnabled,
      isProjectShown: discordShowProject,
      project: frontProject(),
      // Read here so a rename (a new settings.labels entry) re-runs this effect
      // and re-publishes the friendly name, not the stale temp-<stamp> folder.
      labels: settings.labels,
      kind: discordProjectKind
    });
  });

  // Auto-name a temp workspace once the agent has produced real work
  // (lib/agent/auto-name): after a few distinct files change, ask the agent (or a
  // heuristic) for a friendly label. Fires once per workspace; label-only.
  const autoNamer = createAutoNamer({
    isOptedOut: () => settings.prefs.autoNameTemp === false,
    labelOf: path => settings.labels[path],
    activeAgentCommand: () => windowSessions.activeSession?.agent.command ?? ""
  });
  onMount(() => {
    autoNamer.start();
    return () => autoNamer.dispose();
  });

  // Send-from-IDE bridge (lib/terminal/input/send-shortcut): copy in any external editor, press
  // the global shortcut, and the clipboard lands in the active agent's input.
  onMount(() => {
    registerSendShortcut({
      activeId: () => windowSessions.activeId,
      activeLabel: () => windowSessions.activeSession?.agent.label ?? "agent"
    });
    return () => unregisterSendShortcut();
  });
}
