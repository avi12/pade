<script lang="ts">
  import { Command, EventType, onMessage, sendMessage } from "@/lib/ipc/messaging";
  import type { StoredChatRegistry } from "@/lib/session/chat-registry";
  import { sidebarProjects } from "@/lib/session/chat-registry";
  import { sendSettingsMessage } from "@/lib/settings/settings-requests";
  import { settings } from "@/lib/settings/settings.svelte";
  import { firstListedProject, sidebarProjectOrder, sidebarSections } from "@/lib/shell/sidebar/sidebar-sections";
  import SidebarHeader from "@/lib/shell/sidebar/SidebarHeader.svelte";
  import SidebarProjectList from "@/lib/shell/sidebar/SidebarProjectList.svelte";
  import { PROJECT_SEARCH_ID } from "@/lib/shell/window-shortcut-actions";
  import { recordedConversations, refreshRecordedConversations } from "@/lib/stores/session/recordedConversations.svelte";
  import { sessionLabel } from "@/lib/stores/session/sessionLabels.svelte";
  import { invalidateProjectBranches } from "@/lib/stores/workspace/projectBranches.svelte";
  import { revalidateProjectKinds } from "@/lib/stores/workspace/projectKinds.svelte";
  import { revalidateProjectLanguages } from "@/lib/stores/workspace/projectLanguages.svelte";
  import { WindowMode } from "@/lib/types";
  import type { AgentSession, RecordedConversation, WindowInfo } from "@/lib/types";
  import { isNewWindowClick, otherWindowWith } from "@/lib/workspace/open-windows";
  import FolderWatch from "@/lib/workspace/switcher/FolderWatch.svelte";
  import { isCurrentProject } from "@/lib/workspace/switcher/switcher-rows";
  import SwitcherDialogs from "@/lib/workspace/switcher/SwitcherDialogs.svelte";
  import SwitcherFilter from "@/lib/workspace/switcher/SwitcherFilter.svelte";
  import type { Relocator } from "@/lib/workspace/workspace-relocate";
  import type { UnlistenFn } from "@tauri-apps/api/event";
  import { onDestroy, onMount } from "svelte";

  // The left rail - the one place for projects. A search on top (Ctrl P), then
  // every project as the switcher's rich row (pins first, then this window's
  // project, the recents and any other holding chats) with its chats grouped
  // under it - live sessions for the open project, the persisted ones
  // elsewhere, reopened on their own conversations when picked. The project
  // actions (picker, rename, new windows) hang off the head.
  const {
    registry,
    project,
    sessions,
    activeId,
    onopenproject,
    onswitch,
    ondeleteproject,
    onrenamefolder,
    onselectchat,
    onresumeconversation,
    ondeletechat,
    onstopchat,
    onnewchat
  }: {
    registry: StoredChatRegistry;
    /** The project this window has open - its chats are the live sessions.
     *  Empty while the picker is up. */
    project: string;
    sessions: AgentSession[];
    activeId: string | null;
    /** Switch this window to `project` (in place). */
    onopenproject: (project: string) => void;
    /** Open the full picker - absent where the picker is already on screen. */
    onswitch?: () => void;
    /** Delete a project's directory from disk - resolves once removed (or
     *  throws its message for the still-open prompt). */
    ondeleteproject: (project: string) => Promise<void>;
    /** Rename a project's folder on disk where it stands. */
    onrenamefolder: Relocator["renameFolder"];
    onselectchat: (target: {
      project: string;
      chatId: string;
    }) => void;
    onresumeconversation: (target: {
      project: string;
      conversation: RecordedConversation;
    }) => void;
    ondeletechat: (target: {
      project: string;
      chatId: string;
    }) => void;
    /** End a live chat's agent session the way closing its tab does. */
    onstopchat: (target: {
      project: string;
      chatId: string;
    }) => void;
    onnewchat: (project: string) => void;
  } = $props();

  let filter = $state("");
  // The other PADE windows - a project one of them has open is focused there
  // rather than opened twice, and can't have its folder renamed from here.
  let windowRows = $state<WindowInfo[]>([]);
  // The instant every row's "last accessed" age is measured against - renewed
  // whenever the rail is looked at again, so the ages stay a glance, not a clock.
  let now = $state(Date.now());

  // The row prompts (delete directory, relabel, rename folder): a target opens
  // its prompt; the parent only performs the removal or the move.
  let deleteTarget = $state<string | null>(null);
  let relabelTarget = $state<string | null>(null);
  let renameFolderTarget = $state<string | null>(null);

  const projectOrder = $derived(
    sidebarProjectOrder({
      current: project,
      pinned: settings.pinnedProjects,
      recent: settings.recentProjects,
      registry
    })
  );

  const liveLabels = $derived(
    Object.fromEntries(sessions.map(session => [session.id, sessionLabel(session.id)] as const))
  );

  const groups = $derived(
    sidebarProjects({
      registry,
      projectOrder,
      currentProject: project,
      liveSessions: sessions,
      liveLabels,
      recordedConversations
    })
  );

  const sections = $derived(
    sidebarSections({
      groups,
      pinned: settings.pinnedProjects,
      query: filter,
      labels: settings.labels
    })
  );

  // Re-read the agents' recorded conversations when the listed projects
  // change, or a chat opens or closes (a closed chat lives on as a recorded
  // conversation). One joined key of both, so a registry save that leaves
  // them as they were doesn't re-read anything.
  const refreshKey = $derived([sessions.length, ...projectOrder].join("\n"));
  $effect(() => {
    const [, ...projects] = refreshKey.split("\n");
    refreshRecordedConversations(projects);
  });

  async function loadOpenWindows() {
    try {
      windowRows = await sendMessage(Command.windowList);
    } catch (error) {
      console.warn("pade: keeping the last-known open windows", error);
    }
  }

  /** Bring a project up: this window's own is already here, one another window
   *  has open is focused there, Ctrl/Cmd/Shift opens a new window, and anything
   *  else switches this window to it. */
  async function pickProject({ project: picked, opensNewWindow }: {
    project: string;
    opensNewWindow: boolean;
  }) {
    if (isCurrentProject({
      project: picked,
      current: project
    })) {
      return;
    }

    const openWindow = otherWindowWith({
      windowRows,
      path: picked
    });
    if (openWindow) {
      await sendMessage(Command.windowFocusLabel, { label: openWindow.label });
      return;
    }

    if (opensNewWindow) {
      await sendMessage(Command.windowCreate, {
        mode: WindowMode.enum.open,
        path: picked
      });
      return;
    }

    onopenproject(picked);
  }

  onMount(loadOpenWindows);

  // Keep the branch chips honest: a branch switch or git init in any listed
  // project re-asks for the visible rows.
  let unlistenGitState: UnlistenFn | undefined;
  onMount(async () => {
    unlistenGitState = await onMessage(EventType.gitState, invalidateProjectBranches);
  });
  onDestroy(() => unlistenGitState?.());

  // A window opened, closed or moved to another project elsewhere - the
  // "open in another window" marks follow.
  let unlistenWindowsChanged: UnlistenFn | undefined;
  onMount(async () => {
    unlistenWindowsChanged = await onMessage(EventType.windowsChanged, loadOpenWindows);
  });
  onDestroy(() => unlistenWindowsChanged?.());
</script>

<svelte:document
  onvisibilitychange={async () => {
    if (document.hidden) {
      return;
    }

    // Looked at again: fresh ages, branches, kinds and open windows.
    now = Date.now();
    invalidateProjectBranches();
    revalidateProjectKinds();
    revalidateProjectLanguages();
    await loadOpenWindows();
  }}
/>

<nav class="sidebar" aria-label="Projects and chats">
  <SidebarHeader {onswitch} />

  <div class="search">
    <!-- Ctrl P focuses it from anywhere (a window shortcut) -->
    <SwitcherFilter
      id={PROJECT_SEARCH_ID}
      onenter={async () => {
        const first = firstListedProject(sections);
        if (first !== undefined) {
          await pickProject({
            project: first,
            opensNewWindow: false
          });
        }
      }}
      bind:value={filter}
    />
  </div>

  <SidebarProjectList
    {activeId}
    isFiltered={filter.trim() !== ""}
    labels={settings.labels}
    {now}
    onclearrecent={async () => {
      await sendSettingsMessage(Command.workspaceClearRecent);
    }}
    {ondeletechat}
    ondeleteproject={target => {
      deleteTarget = target;
    }}
    {onnewchat}
    onpick={async ({ project: picked, event }) => await pickProject({
      project: picked,
      opensNewWindow: isNewWindowClick(event)
    })}
    onrelabel={target => {
      relabelTarget = target;
    }}
    onremove={async target => {
      await sendSettingsMessage(Command.workspaceRemoveRecent, { path: target });
    }}
    onrenamefolder={target => {
      renameFolderTarget = target;
    }}
    onreorderpins={async paths => {
      await sendSettingsMessage(Command.workspaceSetPinnedOrder, { paths });
    }}
    {onresumeconversation}
    {onselectchat}
    {onstopchat}
    ontogglepin={async target => {
      await sendSettingsMessage(Command.workspaceSetPinned, target);
    }}
    {sections}
    {windowRows}
  />

  <SwitcherDialogs
    labels={settings.labels}
    ondelete={ondeleteproject}
    {onrenamefolder}
    bind:deleteTarget
    bind:relabelTarget
    bind:renameFolderTarget
  />

  <!-- A listed folder deleted or renamed outside PADE leaves the rail -->
  <FolderWatch
    isActive
    onvanish={async () => {
      await sendSettingsMessage(Command.workspacePrune);
    }}
    projects={[...settings.pinnedProjects, ...settings.recentProjects]}
  />
</nav>

<style>
  /* The left rail - a fixed-width column that never squeezes the terminal:
     the head and search stay put while the project list scrolls on its own. */
  .sidebar {
    display: flex;
    flex: none;
    flex-direction: column;
    min-block-size: 0;
    inline-size: 272px;
    border-inline-end: 1px solid var(--outline);
    background: var(--surface-1);
  }

  .search {
    flex: none;
    padding-block-end: 4px;
    padding-inline: 6px;
  }

  @media (width <= 720px) {
    .sidebar {
      inline-size: 208px;
    }
  }
</style>
