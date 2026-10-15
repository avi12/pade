// The background services a window runs for as long as it lives, each keyed on
// the open project where it matters: the backend streams it subscribes to, the
// project watch, the workspace-member census, the branch pill's refresh, temp
// workspace auto-naming, the send-from-IDE shortcut and Discord presence. Each
// service owns its store; this only starts them and follows the project.

import { createAutoNamer } from "@/lib/agent/auto-name";
import { Command, EventType, onMessage, sendMessage } from "@/lib/ipc/messaging";
import { effective } from "@/lib/settings/prefs.svelte";
import { sendSettingsMessage } from "@/lib/settings/settings-requests";
import { settings } from "@/lib/settings/settings.svelte";
import type { createActiveRepository } from "@/lib/shell/activeRepository.svelte";
import { updateDiscordPresence } from "@/lib/shell/discord-presence";
import type { WindowSessions } from "@/lib/shell/windowSessions.svelte";
import { ensureChoiceAttention } from "@/lib/stores/session/sessionAttention.svelte";
import { ensureRunnerListeners } from "@/lib/stores/tasks/runners.svelte";
import { initTaskRunDetection, refreshTaskRunDetection } from "@/lib/stores/tasks/taskRuns.svelte";
import { projectKind, requestProjectKind } from "@/lib/stores/workspace/projectKinds.svelte";
import {
  activeMemberIn,
  loadWorkspaceMembers,
  refreshMemberBranches
} from "@/lib/stores/workspace/workspaceMembers.svelte";
import { registerSendShortcut, unregisterSendShortcut } from "@/lib/terminal/send-shortcut";
import { onMount, untrack } from "svelte";

export function startWindowServices({ project, windowSessions, repository, isWindowFocused }: {
  /** The window's open project; empty while none is. */
  project: () => string;
  windowSessions: WindowSessions;
  repository: ReturnType<typeof createActiveRepository>;
  /** Whether this window has focus - only the focused one publishes presence. */
  isWindowFocused: () => boolean;
}) {
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

  // Reflect known tasks the agent runs as "running" in the Tasks panel.
  onMount(async () => {
    await initTaskRunDetection(project);
  });

  // Adopt settings another window changed - it deleted a workspace, renamed one,
  // toggled a pin - so this window's switcher and picker never list a project
  // that is already gone. Best-effort because an event handler can't await: a
  // failed re-read leaves the settings already adopted in place.
  let unlistenSettings: (() => void) | undefined;
  async function subscribeToSettingsChanges() {
    unlistenSettings = await onMessage(EventType.settingsChanged, async () => {
      await Promise.allSettled([sendSettingsMessage(Command.settingsGet)]);
    });
  }
  onMount(() => {
    subscribeToSettingsChanges();
    return () => unlistenSettings?.();
  });

  // Watch the open project's files app-wide - not only while the Change Feed is
  // open - so the Tasks panel auto-updates on a manifest/script edit, and the
  // task-run and auto-name subscribers see changes from the moment a project
  // opens. Keyed on the project so it re-roots on an in-window switch; the
  // backend is idempotent, so a repeat call for the same root is a no-op.
  $effect(() => {
    const root = project();
    if (!root) {
      return;
    }

    sendMessage(Command.watchStart, { root });
    refreshTaskRunDetection();
  });

  // What this workspace is made of - nested checkouts and declared packages, as
  // one list (stores/workspaceMembers). Keyed on the project so an in-window
  // switch re-discovers them; the member switcher, the branch pill and the
  // Change Feed's grouping all read that one list.
  $effect(() => {
    if (!project()) {
      return;
    }

    untrack(discoverWorkspaceMembers);
  });

  async function discoverWorkspaceMembers() {
    await loadWorkspaceMembers(project());
  }

  // The branch pill reports the member the switcher points at, so picking a
  // nested checkout re-reads its branch, its branch list and its remote.
  $effect(() => {
    const member = activeMemberIn(project());
    if (!member) {
      return;
    }

    untrack(repository.load);
  });

  // Keep the branch pill honest: a branch switch, git init, or remote change in
  // the open project re-fetches what it shows (git://state carries no payload).
  let unlistenGitState: (() => void) | undefined;
  async function subscribeToGitState() {
    unlistenGitState = await onMessage(EventType.gitState, async () => {
      await Promise.all([repository.load(), refreshMemberBranches()]);
    });
  }
  onMount(() => {
    subscribeToGitState();
    return () => unlistenGitState?.();
  });

  // ── Discord Rich Presence ────────────────────────────────────────────────────
  // Broadcast "Playing PADE" (opt-in), optionally naming the open project. The
  // state→bridge mapping lives in lib/shell/discord-presence (SoC). Both flags resolve
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
  const discordProjectKind = $derived(projectKind(project()) ?? undefined);
  $effect(() => {
    const shown = project();
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
      project: project(),
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
    currentProject: project,
    isOptedOut: () => settings.prefs.autoNameTemp === false,
    labelOf: path => settings.labels[path],
    activeAgentCommand: () => windowSessions.activeSession?.agent.command ?? ""
  });
  onMount(() => {
    autoNamer.start();
    return () => autoNamer.dispose();
  });

  // Send-from-IDE bridge (lib/terminal/send-shortcut): copy in any external editor, press
  // the global shortcut, and the clipboard lands in the active agent's input.
  onMount(() => {
    registerSendShortcut({
      activeId: () => windowSessions.activeId,
      activeLabel: () => windowSessions.activeSession?.agent.label ?? "agent"
    });
    return () => unregisterSendShortcut();
  });
}
