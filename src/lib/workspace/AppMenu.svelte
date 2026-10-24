<script lang="ts">
  import { Command, EventType, onMessage, sendMessage } from "@/lib/ipc/messaging";
  import { sendSettingsMessage } from "@/lib/settings/settings-requests";
  import { invalidateProjectBranches } from "@/lib/stores/workspace/projectBranches.svelte";
  import { revalidateProjectKinds } from "@/lib/stores/workspace/projectKinds.svelte";
  import { revalidateProjectLanguages } from "@/lib/stores/workspace/projectLanguages.svelte";
  import { WindowMode } from "@/lib/types";
  import type { AddRootOutcome, WindowInfo } from "@/lib/types";
  import { Key } from "@/lib/ui/keys";
  import { arrowFocus } from "@/lib/ui/roving-menu";
  import { ToggleState } from "@/lib/ui/toggle-state";
  import { normalizePath } from "@/lib/util/paths";
  import { otherWindowWith } from "@/lib/workspace/open-windows";
  import SaveWorkspaceCard from "@/lib/workspace/SaveWorkspaceCard.svelte";
  import FolderWatch from "@/lib/workspace/switcher/FolderWatch.svelte";
  import { runListTransition } from "@/lib/workspace/switcher/list-transition";
  import OpenWindowList from "@/lib/workspace/switcher/OpenWindowList.svelte";
  import ProjectList from "@/lib/workspace/switcher/ProjectList.svelte";
  import {
    isCurrentProject,
    isSameMembership,
    matchesProjectFilter,
    windowRowsInOrder
  } from "@/lib/workspace/switcher/switcher-rows";
  import type { ListedProjects } from "@/lib/workspace/switcher/switcher-rows";
  import SwitcherActions from "@/lib/workspace/switcher/SwitcherActions.svelte";
  import SwitcherDialogs from "@/lib/workspace/switcher/SwitcherDialogs.svelte";
  import SwitcherFilter from "@/lib/workspace/switcher/SwitcherFilter.svelte";
  import SwitcherTrigger from "@/lib/workspace/switcher/SwitcherTrigger.svelte";
  import type { Relocator } from "@/lib/workspace/workspace-relocate";
  import type { UnlistenFn } from "@tauri-apps/api/event";
  import { onDestroy, onMount, tick, untrack } from "svelte";

  // The project switcher that leads the top bar. It lists every open PADE window
  // (jump between them, or cycle with Ctrl+Alt+[ / ]), then is a fast way to
  // switch THIS window to another project: type to filter, or click a pinned/recent
  // row. New windows and the full picker sit below. In-window switches funnel
  // through `onopen`; pin/remove/reorder persist via parent callbacks and the
  // shared settings authority; window focus + the list use the bridge directly.
  // Up/Down walk the project items in the order they read - window rows, the
  // filter field, then pinned/recent rows: every stop is tagged
  // `data-arrow-stop`, and `arrowFocus` moves focus between them.
  const {
    path,
    label,
    isTemp,
    remoteRepository,
    roots,
    recentProjects,
    pinnedProjects,
    labels,
    onopen,
    onswitch,
    ontogglepin,
    onclearrecent,
    onremoverecent,
    ondelete,
    onrenamefolder,
    onreorderpins,
    onsavetemp,
    onaddroot
  }: {
    path: string;
    label: string;
    isTemp: boolean;
    /** The repository the trigger's Ctrl-click opens on its remote; null when
     *  it has none. */
    remoteRepository: string | null;
    /** The saved project roots - destinations the temp workspace can be saved into. */
    roots: string[];
    recentProjects: string[];
    pinnedProjects: string[];
    labels: Record<string, string>;
    /** Switch this window to `path` (in place). */
    onopen: (path: string) => void;
    /** Open the full picker (browse every root, clone, open a folder). */
    onswitch: () => void;
    /** Pin or unpin a project - persisted by the parent into shared settings.
     *  Resolves once settings are updated, so the switcher can wrap it in a view
     *  transition and morph the row to its new section. */
    ontogglepin: (target: {
      path: string;
      pinned: boolean;
    }) => Promise<void>;
    /** Clear the recent-projects history (pins survive). Resolves when done. */
    onclearrecent: () => Promise<void>;
    /** Forget one project from the switcher (recents + pins); folder untouched.
     *  Resolves when done. */
    onremoverecent: (path: string) => Promise<void>;
    /** Delete a project's directory from disk - performs the removal and resolves
     *  (or throws its message). The switcher owns the confirmation UI, so it stays
     *  open behind the prompt and animates the row out when this resolves. */
    ondelete: (path: string) => Promise<void>;
    /** Rename a project's folder on disk where it stands - resolves to its new
     *  path once it moved (or throws its reason). */
    onrenamefolder: Relocator["renameFolder"];
    /** Persist a drag-reordered pin order. */
    onreorderpins: (paths: string[]) => void;
    /** Save the current temp workspace as a real project: rename it into `root`
     *  (one of the saved roots) under `name`. Resolves once relocated. */
    onsavetemp: (target: {
      name: string;
      root: string;
    }) => Promise<void>;
    /** Add an existing folder as a project root - the no-roots save path.
     *  Persisted by the parent into shared settings. */
    onaddroot: (path: string) => Promise<AddRootOutcome>;
  } = $props();

  const MENU_ID = "application-menu";
  const MENU_ANCHOR = "--application-menu-anchor";
  const FILTER_ID = "application-menu-query";
  const POPOVER_OPEN_SELECTOR = ":popover-open";
  const ARROW_STOP_SELECTOR = "[data-arrow-stop]";
  const SHORTCUT_KEY = "p";

  let filter = $state("");
  const filterQuery = $derived(filter.trim());
  // The open-window list is fetched when the menu opens. Branch chips and language
  // icons load independently, only as their rows become visible.
  let windowRows = $state<WindowInfo[]>([]);
  // Whether the popover is open - the moment a row first has a real width, so the
  // per-row truncation tooltips (re)measure then rather than while hidden.
  let isMenuOpen = $state(false);
  // When the menu last opened - the instant every row's "last accessed" age is
  // measured against, so the ages are fresh on each open without a ticking clock.
  let openedAt = $state(Date.now());
  // The rows containers, so list mutations animate through a view transition
  // scoped to *just* that element (never the document - that would snapshot the
  // live-repainting terminal and ghost it). The open windows sit outside the
  // pinned/recent list, so they morph within their own container.
  let elSwitchList = $state<HTMLElement | null>(null);
  let elWindowList = $state<HTMLElement | null>(null);

  // The row prompts (delete directory, relabel, rename folder) nest inside this
  // menu so the switcher stays visible behind them; a target opens its prompt.
  // The parent only performs the removal.
  let deleteTarget = $state<string | null>(null);
  let relabelTarget = $state<string | null>(null);
  let renameFolderTarget = $state<string | null>(null);

  // What the list shows. It follows the settings, but a change to WHICH
  // projects are listed plays through the scoped transition whoever made it -
  // a row removed here, a pin, or a folder another window found deleted - so a
  // row never just blinks out. A reorder (or a closed menu) is taken as is.
  let listed = $state<ListedProjects>(
    untrack(() => ({
      pinned: pinnedProjects,
      recent: recentProjects
    }))
  );

  async function followListed(next: ListedProjects) {
    const isReshaped = !isSameMembership({
      before: listed,
      after: next
    });
    if (!isMenuOpen || !isReshaped) {
      listed = next;
      return;
    }

    await runListTransition({
      elList: elSwitchList,
      async mutate() {
        listed = next;
      }
    });
  }

  $effect(() => {
    const next = {
      pinned: pinnedProjects,
      recent: recentProjects
    };
    untrack(() => followListed(next));
  });

  const pinnedSet = $derived(new Set(listed.pinned.map(normalizePath)));
  // Recents minus anything already pinned, so a project shows in one section only.
  const recentsOnly = $derived(listed.recent.filter(project => !pinnedSet.has(normalizePath(project))));
  const pinnedShown = $derived(
    listed.pinned.filter(project => matchesProjectFilter({
      project,
      query: filterQuery,
      labels
    }))
  );
  const recentShown = $derived(
    recentsOnly.filter(project => matchesProjectFilter({
      project,
      query: filterQuery,
      labels
    }))
  );

  // Fetch the open windows. Branches and language kinds are deliberately absent:
  // both load per row, only as rows become visible (see visiblePathBatch).
  async function loadOpenWindows() {
    try {
      windowRows = await sendMessage(Command.windowList);
    } catch {
    // Keep the last-known rows; the next open or change event retries.
    }
  }

  /** Bring up the window that already has this project open, the way its row in
   *  the open-windows list does. False when no other window has it. */
  async function focusWindowWith(project: string) {
    const openWindow = otherWindowWith({
      windowRows,
      path: project
    });
    if (!openWindow) {
      return false;
    }

    await sendMessage(Command.windowFocusLabel, { label: openWindow.label });
    return true;
  }

  function hide() {
    const elMenu = document.getElementById(MENU_ID);
    if (elMenu?.matches(POPOVER_OPEN_SELECTOR)) {
      elMenu.hidePopover();
    }
  }

  // Focus the filter as the menu opens: it's ready to type, and - crucially -
  // the trigger no longer keeps focus, so a later keypress (holding Shift for a
  // shift-click) can't flip `:focus-visible` on and paint a stray ring on it. The
  // rAF waits for the popover's top layer to lay out before the field is focusable.
  function focusFilter() {
    requestAnimationFrame(() => document.getElementById(FILTER_ID)?.focus());
  }

  // Ctrl P from anywhere opens the switcher and focuses its filter, matching the
  // shortcut the trigger and the search field advertise. It runs in the capture
  // phase (like pane-shortcuts) so it wins over the terminal - xterm would
  // otherwise swallow Ctrl P and send it to the agent - and the browser's print
  // dialog, neither of which sees the chord once it's consumed here.
  function openViaShortcut(e: KeyboardEvent) {
    const isCtrlP =
      (e.ctrlKey || e.metaKey) && !e.shiftKey && !e.altKey && e.key.toLowerCase() === SHORTCUT_KEY;
    if (!isCtrlP) {
      return;
    }

    e.preventDefault();
    e.stopImmediatePropagation();
    const elMenu = document.getElementById(MENU_ID);
    if (elMenu && !elMenu.matches(POPOVER_OPEN_SELECTOR)) {
      elMenu.showPopover();
    }

    focusFilter();
  }

  onMount(() => {
    addEventListener("keydown", openViaShortcut, { capture: true });
    return () => removeEventListener("keydown", openViaShortcut, { capture: true });
  });

  // Keep the branch chips honest while the switcher is on screen: a branch
  // switch or git init in any listed project re-asks for the visible rows. A
  // closed menu skips it - opening invalidates them anyway.
  let unlistenGitState: UnlistenFn | undefined;
  onMount(async () => {
    unlistenGitState = await onMessage(EventType.gitState, () => {
      if (isMenuOpen) {
        invalidateProjectBranches();
      }
    });
  });
  onDestroy(() => unlistenGitState?.());

  // Reflect open-windows changes made in OTHER windows live (their reorder lands
  // → this switcher morphs to match, with the same scoped morph the pinned/recent
  // list uses), so two open menus never disagree.
  let unlistenWindowsChanged: UnlistenFn | undefined;
  onMount(async () => {
    unlistenWindowsChanged = await onMessage(EventType.windowsChanged, async () => {
      if (!isMenuOpen) {
        return;
      }

      let openWindows: WindowInfo[];
      try {
        openWindows = await sendMessage(Command.windowList);
      } catch {
        return;
      }

      await runListTransition({
        elList: elWindowList,
        async mutate() {
          windowRows = openWindows;
          await tick();
        }
      });
    });
  });
  onDestroy(() => unlistenWindowsChanged?.());
</script>

<!-- Close the menu on Escape from ANYWHERE, including while the terminal holds
     focus: xterm's textarea preventDefaults Escape to forward it to the agent,
     which swallows the popover's native close-signal. A capture-phase window
     handler runs before xterm, so it wins while the menu is open. -->
<svelte:window
  onkeydowncapture={e => {
    if (isMenuOpen && e.key === Key.Escape) {
      e.preventDefault();
      e.stopPropagation();
      hide();
    }
  }}
/>

<span class="menu-host">
  <SwitcherTrigger {isTemp} {label} menuAnchor={MENU_ANCHOR} menuId={MENU_ID} {remoteRepository} />

  <div
    id={MENU_ID}
    style:position-anchor={MENU_ANCHOR}
    class="menu popover-menu" {@attach arrowFocus(ARROW_STOP_SELECTOR)}
    ontoggle={async e => {
      isMenuOpen = (e as ToggleEvent).newState === ToggleState.Open;

      if (isMenuOpen) {
        openedAt = Date.now();
        focusFilter();
        invalidateProjectBranches();
        revalidateProjectKinds();
        revalidateProjectLanguages();
        await loadOpenWindows();
      }
    }}
    popover
    role="menu"
  >
    <!-- Save the throwaway workspace as a real project: name + destination root.
         Design's "Save this workspace" card, shown only for a temp project. -->
    {#if isTemp}
      <SaveWorkspaceCard
        {isMenuOpen}
        {label}
        {onaddroot}
        onrelabel={() => {
          relabelTarget = path;
        }}
        onsaved={hide}
        {onsavetemp}
        {roots}
      />
      <div class="menu-divider"></div>
    {/if}

    {#if windowRows.length > 0}
      <OpenWindowList
        {labels}
        onfocuswindow={async windowLabel => {
          hide();
          await sendMessage(Command.windowFocusLabel, { label: windowLabel });
        }}
        onreorder={async windowLabels => {
          // Persist the new order to the one backend source (which drives both
          // this list and the Ctrl+Alt+[ / ] cycle), reflect it at once, then
          // re-fetch so the rows match backend truth.
          windowRows = windowRowsInOrder({
            windowRows,
            order: windowLabels
          });
          await sendMessage(Command.windowReorder, { labels: windowLabels });
          await loadOpenWindows();
        }}
        {windowRows}
        bind:elList={elWindowList}
      />
      <div class="menu-divider"></div>
    {/if}

    <SwitcherFilter
      id={FILTER_ID}
      onenter={async () => {
        const first = pinnedShown[0] ?? recentShown[0];
        if (!first) {
          return;
        }

        hide();

        const isAlreadyOpen = isCurrentProject({
          project: first,
          current: path
        }) || await focusWindowWith(first);
        if (!isAlreadyOpen) {
          onopen(first);
        }
      }}
      bind:value={filter}
    />

    <ProjectList
      isFiltered={filterQuery !== ""}
      {labels}
      {onclearrecent}
      ondelete={project => {
        deleteTarget = project;
      }}
      onpick={async ({ project, event }) => {
        hide();

        if (isCurrentProject({
          project,
          current: path
        }) || await focusWindowWith(project)) {
          return;
        }

        // Ctrl/Cmd/Shift opens the project in a new window instead of
        // switching this one.
        const opensNewWindow = event.ctrlKey || event.metaKey || event.shiftKey;
        if (opensNewWindow) {
          await sendMessage(Command.windowCreate, {
            mode: WindowMode.enum.open,
            path: project
          });
          return;
        }

        onopen(project);
      }}
      onrelabel={project => {
        relabelTarget = project;
      }}
      onremove={onremoverecent}
      onrenamefolder={project => {
        renameFolderTarget = project;
      }}
      {onreorderpins}
      {ontogglepin}
      {openedAt}
      {path}
      {pinnedShown}
      {recentShown}
      {windowRows}
      bind:elList={elSwitchList}
    />

    <div class="menu-divider"></div>

    <SwitcherActions
      {labels}
      onrenamefolder={() => {
        renameFolderTarget = path;
      }}
      onspawn={async mode => {
        // Dismiss the menu once the window exists, so it doesn't linger over it.
        await sendMessage(Command.windowCreate, { mode });
        hide();
      }}
      onswitch={() => {
        hide();
        onswitch();
      }}
      {path}
    />

    <SwitcherDialogs
      {labels}
      {ondelete}
      {onrenamefolder}
      bind:deleteTarget
      bind:relabelTarget
      bind:renameFolderTarget
    />

    <!-- A listed folder deleted or renamed outside PADE closes out of the list
         with the same morph a removal from the menu plays -->
    <FolderWatch
      isActive={isMenuOpen}
      onvanish={async () => {
        await sendSettingsMessage(Command.workspacePrune);
      }}
      projects={[...pinnedProjects, ...recentProjects]}
    />
  </div>
</span>

<style>
  .menu-host {
    display: contents;
  }

  /* Shell comes from the shared .popover-menu; width, colour and anchor side here. */
  .menu {
    inline-size: 352px;
    max-inline-size: 92vw;
    color: var(--on-surface);
    animation: pop-in 220ms var(--spring);
    position-area: bottom span-right;
  }
</style>
