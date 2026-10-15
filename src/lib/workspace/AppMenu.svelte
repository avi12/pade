<script lang="ts">
  import RelabelDialog from "@/lib/dialogs/RelabelDialog.svelte";
  import RenameFolderDialog from "@/lib/dialogs/RenameFolderDialog.svelte";
  import WorkspaceDeleteDialog from "@/lib/dialogs/WorkspaceDeleteDialog.svelte";
  import Icon from "@/lib/Icon.svelte";
  import { Command, EventType, onMessage, sendMessage } from "@/lib/ipc/messaging";
  import {
    invalidateProjectBranches,
    observeProjectBranch,
    projectBranch
  } from "@/lib/stores/workspace/projectBranches.svelte";
  import { WindowMode } from "@/lib/types";
  import type { AddRootOutcome, WindowInfo } from "@/lib/types";
  import { Axis, beginReorder } from "@/lib/ui/drag-reorder";
  import { Key } from "@/lib/ui/keys";
  import Logo from "@/lib/ui/Logo.svelte";
  import { prefersReducedMotion } from "@/lib/ui/motion";
  import { arrowFocus, rovingMenu } from "@/lib/ui/roving-menu";
  import { ToggleState } from "@/lib/ui/toggle-state";
  import { tooltip, truncationTooltip } from "@/lib/ui/truncation-tooltip";
  import { displayName, isTemporaryWorkspace, normalizePath, shortDisplayName } from "@/lib/util/paths";
  import { openRepositoryOnModifiedClick } from "@/lib/vcs/repository-links";
  import LastAccessed from "@/lib/workspace/LastAccessed.svelte";
  import { isOpenInAnotherWindow } from "@/lib/workspace/open-windows";
  import ProjectKindIcon from "@/lib/workspace/ProjectKindIcon.svelte";
  import SaveWorkspaceCard from "@/lib/workspace/SaveWorkspaceCard.svelte";
  import TempBadge from "@/lib/workspace/TempBadge.svelte";
  import type { Relocator } from "@/lib/workspace/workspace-relocate";
  import type { UnlistenFn } from "@tauri-apps/api/event";
  import { onDestroy, onMount, tick } from "svelte";

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
  // Pixels a row's grip must travel before a press becomes a drag.
  const ROW_DRAG_THRESHOLD = 4;
  // Anything a popover id or view-transition-name can't carry.
  const NON_IDENTIFIER_CHARACTERS = /[^a-zA-Z0-9]/g;

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
  // The rows container, so list mutations animate through a view transition
  // scoped to *just* this element (never the document - that would snapshot the
  // live-repainting terminal and ghost it).
  let elSwitchList = $state<HTMLElement | null>(null);
  let elWindowList = $state<HTMLElement | null>(null);

  // The "Delete directory" confirmation lives inside this menu (a nested popover)
  // so the switcher stays visible behind it; the parent only performs the removal.
  let deleteTarget = $state<string | null>(null);
  let isDeleting = $state(false);
  let deleteError = $state("");

  // The temp workspace whose PADE-only label is being edited - nested over this
  // menu like the delete prompt, so the switcher stays in view behind it.
  let relabelTarget = $state<string | null>(null);
  // The project whose folder is being renamed on disk - nested the same way.
  let renameFolderTarget = $state<string | null>(null);

  const pinnedSet = $derived(new Set(pinnedProjects.map(normalizePath)));
  // Recents minus anything already pinned, so a project shows in one section only.
  const recentsOnly = $derived(recentProjects.filter(project => !pinnedSet.has(normalizePath(project))));

  function matchesFilter(project: string) {
    const query = filterQuery.toLowerCase();
    if (!query) {
      return true;
    }

    return (
      displayName({
        path: project,
        labels
      }).toLowerCase().includes(query) ||
      project.toLowerCase().includes(query)
    );
  }

  const pinnedShown = $derived(pinnedProjects.filter(matchesFilter));
  const recentShown = $derived(recentsOnly.filter(matchesFilter));
  const isFilterUnmatched = $derived(
    filterQuery.length > 0 && pinnedShown.length === 0 && recentShown.length === 0
  );
  // Pins reorder only when there's more than one and no filter narrows the set -
  // the drag engine commits the visible rows, so a filtered subset would drop the
  // hidden pins from the saved order.
  const isPinListReorderable = $derived(filterQuery === "" && pinnedProjects.length > 1);
  // A lone window can't be reordered - mirror the pins guard so the grip only
  // appears when there's more than one open window to drag among.
  const isWindowListReorderable = $derived(windowRows.length > 1);

  function isCurrent(project: string) {
    return normalizePath(project) === normalizePath(path);
  }
  function identifierFrom(text: string) {
    return text.replaceAll(NON_IDENTIFIER_CHARACTERS, "-");
  }
  // Stable, valid popover id/anchor per row kebab (sanitised from the path).
  function rowMenuId(project: string) {
    return `switcher-${identifierFrom(project)}`;
  }
  // Stable, unique view-transition-name per row, so the scoped view transition
  // tracks each row across a list change and morphs it (a moved row glides to its
  // new slot) instead of cross-fading the whole list. The same project keeps the
  // same name across sections, so pinning it glides from Recent to Pinned.
  function rowTransitionName(project: string) {
    return `row-${identifierFrom(project)}`;
  }
  // A separate namespace from `rowTransitionName`, keyed by the window's stable
  // session label: a window and a pinned row can share a project path, and two
  // elements can't carry the same view-transition-name in one transition.
  function windowRowTransitionName(windowLabel: string) {
    return `window-row-${identifierFrom(windowLabel)}`;
  }

  // Fetch the open windows. Branches and language kinds are deliberately absent:
  // both load per row, only as rows become visible (see visiblePathBatch).
  async function loadOpenWindows() {
    try {
      windowRows = await sendMessage(Command.windowList);
    } catch {
    // Keep the last-known rows; the next open or change event retries.
    }
  }

  // Live cross-window sync: another window changed the open-windows set/order (a
  // drag-reorder landed there), so reflect it here with the same scoped morph the
  // pinned/recent list uses.
  async function onWindowsChanged() {
    if (!isMenuOpen) {
      return;
    }

    let openWindows: WindowInfo[];
    try {
      openWindows = await sendMessage(Command.windowList);
    } catch {
      return;
    }

    await animateWindowListChange(async () => {
      windowRows = openWindows;
      await tick();
    });
  }

  // Persist a drag-reordered window order to the one backend source (which drives
  // both this list and the Ctrl+Alt+[ / ] cycle), reflect it at once, then re-fetch
  // so the rows match backend truth. The drag engine hands us the new label order.
  async function reorderWindows(windowLabels: string[]) {
    const byLabel = new Map(windowRows.map(row => [row.label, row]));
    windowRows = windowLabels.flatMap(labelId => {
      const row = byLabel.get(labelId);
      return row ? [row] : [];
    });
    await sendMessage(Command.windowReorder, { labels: windowLabels });
    await loadOpenWindows();
  }

  function hide() {
    const elMenu = document.getElementById(MENU_ID);
    if (elMenu?.matches(POPOVER_OPEN_SELECTOR)) {
      elMenu.hidePopover();
    }
  }

  // Jump this window to a project (or, with Ctrl/Cmd/Shift held, open it in a new
  // window instead of switching this one).
  async function pick({ project, event }: {
    project: string;
    event: MouseEvent;
  }) {
    hide();

    if (isCurrent(project)) {
      return;
    }

    const opensNewWindow = event.ctrlKey || event.metaKey || event.shiftKey;
    if (opensNewWindow) {
      await sendMessage(Command.windowCreate, {
        mode: WindowMode.enum.open,
        path: project
      });
      return;
    }

    onopen(project);
  }

  // Focus the filter as the menu opens: it's ready to type, and - crucially -
  // the trigger no longer keeps focus, so a later keypress (holding Shift for a
  // shift-click) can't flip `:focus-visible` on and paint a stray ring on it. The
  // rAF waits for the popover's top layer to lay out before the field is focusable.
  function focusFilter() {
    requestAnimationFrame(() => document.getElementById(FILTER_ID)?.focus());
  }

  // Spawn a fresh window and dismiss the menu so it doesn't linger over the new
  // one. `path` is optional - omitted for empty/temp modes.
  async function spawn(args: {
    mode: WindowMode;
    path?: string;
  }) {
    await sendMessage(Command.windowCreate, args);
    hide();
  }

  // Run a list mutation (pin/unpin/remove/delete/clear - each an async settings
  // update that reshapes the rows) inside a view transition scoped to *just* the
  // rows container, so a row morphs - one moved between sections glides to its new
  // place - instead of snapping. Scoped to this element, never `document`, so it
  // never snapshots the live-repainting terminal (which would ghost). Reduced
  // motion, or an engine without the scoped API, runs the mutation directly.
  type ScopedTransitionList = HTMLElement & {
    startViewTransition?: (callback: () => Promise<void>) => { updateCallbackDone: Promise<void> };
  };

  async function runListTransition({ elList, mutate }: {
    elList: ScopedTransitionList | null;
    mutate: () => Promise<void>;
  }) {
    if (prefersReducedMotion() || !elList?.startViewTransition) {
      await mutate();
      return;
    }

    // Resolve once the DOM has updated (so a caller like the delete flow can close
    // its dialog then) - the transition itself keeps animating after that.
    const transition = elList.startViewTransition(async () => {
      await mutate();
      await tick();
    });
    await transition.updateCallbackDone;
  }

  async function animateListChange(mutate: () => Promise<void>) {
    await runListTransition({
      elList: elSwitchList,
      mutate
    });
  }

  // The Open-windows list gets the same scoped morph as the pinned/recent list,
  // so a reorder animates in place. Its rows live outside `elSwitchList`, so the
  // transition is scoped to their own container.
  async function animateWindowListChange(mutate: () => Promise<void>) {
    await runListTransition({
      elList: elWindowList,
      mutate
    });
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
  // → this switcher morphs to match), so two open menus never disagree.
  let unlistenWindowsChanged: UnlistenFn | undefined;
  onMount(async () => {
    unlistenWindowsChanged = await onMessage(EventType.windowsChanged, () => onWindowsChanged());
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
  <button
    class="trigger menu-trigger"
    {@attach openRepositoryOnModifiedClick({ project: path })}
    aria-haspopup="menu"
    aria-label={`Switch project · Ctrl P · Ctrl-click opens repository - ${label}`}
    data-tooltip="Switch project · Ctrl-click opens repository"
    popovertarget={MENU_ID}
  >
    <Logo size={18} />
    <span class="stack">
      <span class="eyebrow">Project</span>
      <span class="name">
        {label}
        {#if isTemp}
          <TempBadge />
        {/if}
      </span>
    </span>
    <span class="caret" aria-hidden="true">▾</span>
  </button>

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

    <!-- Open PADE windows - in creation order, which is also the cycle order for
       Ctrl+Alt+[ / ]. Click a non-current one to focus its window. -->
    {#if windowRows.length > 0}
      <div class="menu-separator">Open windows</div>
      <!-- Own container so the scoped view transition (animateWindowListChange)
           morphs just these rows when the order changes - locally or when another
           window's reorder broadcasts. -->
      <div bind:this={elWindowList} class="open-window-list">
        {#each windowRows as windowRow (windowRow.label)}
          <!-- Grip (a span, so grabbing it never triggers the button's focus
               onclick) + focus button; data-window-id makes the wrapper the reorder
               engine's drag sibling, view-transition-name lets it glide to its new
               slot instead of the list cross-fading. -->
          <div
            style:view-transition-name={windowRowTransitionName(windowRow.label)}
            class="open-window-row-item"
            data-window-id={windowRow.label}
          >
            {#if isWindowListReorderable}
              <span
                class="grip" {@attach tooltip("Drag to reorder")}
                aria-hidden="true"
                onpointerdown={e => beginReorder({
                  e,
                  itemSelector: "[data-window-id]",
                  idAttribute: "data-window-id",
                  axis: Axis.Vertical,
                  threshold: ROW_DRAG_THRESHOLD,
                  onCommit: labelOrder => reorderWindows(labelOrder)
                })}
              ><Icon name="grip" size={14} /></span>
            {/if}
            <button
              class="open-window-row"
              class:current={windowRow.isCurrent}
              data-arrow-stop
              onclick={async () => {
                if (!windowRow.isCurrent) {
                  hide();
                  await sendMessage(Command.windowFocusLabel, { label: windowRow.label });
                }
              }}
              role="menuitem"
              type="button"
            >
              <ProjectKindIcon path={windowRow.path} />
              <span class="open-window-row-name">{shortDisplayName({
                path: windowRow.path,
                labels
              })}</span>
              {#if isTemporaryWorkspace(windowRow.path)}
                <TempBadge />
              {/if}
              <span class="open-window-row-spacer"></span>
              {#if windowRow.isCurrent}
                <span class="this-window">this window</span>
              {:else}
                <span class="open-window-row-focus" aria-hidden="true"><Icon name="external" size={14} /></span>
              {/if}
            </button>
          </div>
        {/each}
      </div>
      <div class="menu-divider"></div>
    {/if}

    <!-- Filter / quick-switch -->
    <label class="search">
      <span class="lead" aria-hidden="true"><Icon name="search" size={15} /></span>
      <input
        id={FILTER_ID}
        aria-label="Switch project by name or path"
        autocomplete="off"
        data-arrow-stop
        onkeydown={e => {
          if (e.key !== Key.Enter) {
            return;
          }

          const first = pinnedShown[0] ?? recentShown[0];
          if (first) {
            hide();

            if (!isCurrent(first)) {
              onopen(first);
            }
          }
        }}
        placeholder="Switch project by name or path…"
        spellcheck="false"
        bind:value={filter}
      />
      <span class="kbd" aria-hidden="true">Ctrl P</span>
    </label>

    <!-- The row's kebab + options popover, shared by both sections. -->
    {#snippet rowMenu(project: string, isPinned: boolean)}
      {@const menuId = rowMenuId(project)}
      <button
        class="project-row-kebab"
        aria-haspopup="menu"
        aria-label={`Options for ${displayName({
          path: project,
          labels
        })}`}
        popovertarget={menuId}
        type="button"
      ><Icon name="more" size={16} /></button>
      <div id={menuId} class="row-menu popover-menu" {@attach rovingMenu} popover role="menu">
        <button
          class="menu-item" onclick={async () => await animateListChange(() => ontogglepin({
            path: project,
            pinned: !isPinned
          }))}
          popovertarget={menuId}
          popovertargetaction="hide"
          role="menuitem"
          type="button">
          <span class="menu-item-icon"><Icon name="star" size={15} /></span>
          <span>{#if isPinned}
            Unpin from top{:else}Pin to top{/if}</span>
        </button>
        {#if isTemporaryWorkspace(project)}
          <button
            class="menu-item" onclick={() => {
              relabelTarget = project;
            }}
            popovertarget={menuId}
            popovertargetaction="hide"
            role="menuitem"
            type="button">
            <span class="menu-item-icon"><Icon name="pencil" size={15} /></span>
            <span>Relabel in PADE</span>
          </button>
        {/if}
        {#if !isOpenInAnotherWindow({
          windowRows,
          path: project
        })}
          <button
            class="menu-item" onclick={() => {
              renameFolderTarget = project;
            }}
            popovertarget={menuId}
            popovertargetaction="hide"
            role="menuitem"
            type="button">
            <span class="menu-item-icon"><Icon name="folder" size={15} /></span>
            <span>Rename folder…</span>
          </button>
        {/if}
        <button
          class="menu-item" onclick={async () => await animateListChange(() => onremoverecent(project))}
          popovertarget={menuId}
          popovertargetaction="hide"
          role="menuitem"
          type="button">
          <span class="menu-item-icon"><Icon name="close" size={15} /></span>
          <span>Remove from list</span>
        </button>
        <div class="menu-divider"></div>
        <button
          class="menu-item critical" onclick={() => {
            deleteError = "";
            deleteTarget = project;
          }}
          popovertarget={menuId}
          popovertargetaction="hide"
          role="menuitem"
          type="button">
          <span class="menu-item-icon"><Icon name="trash" size={15} /></span>
          <span class="menu-item-body">
            <span>Delete directory</span>
            <span class="menu-item-subtitle">{project}</span>
          </span>
        </button>
      </div>
    {/snippet}

    <!-- The row's main button (logo, name, branch, path), shared by both sections. -->
    {#snippet rowMain(project: string)}
      {@const isCurrentProject = isCurrent(project)}
      {@const branch = projectBranch(project)}
      <button
        class="project-row-main"
        class:current={isCurrentProject}
        {@attach observeProjectBranch({ path: project })}
        aria-checked={isCurrentProject}
        data-arrow-stop
        onclick={async e => await pick({
          project,
          event: e
        })}
        role="menuitemradio"
        type="button"
      >
        <ProjectKindIcon path={project} />
        <span class="project-row-body">
          <span class="project-row-name-line">
            <span class="project-row-name">{displayName({
              path: project,
              labels
            })}</span>
            {#if isTemporaryWorkspace(project)}
              <TempBadge />
            {/if}
          </span>
          <span class="project-row-metadata">
            {#if branch}
              <span class="branch">
                <span class="branch-icon" aria-hidden="true"><Icon name="branch" size={11} /></span>
                {branch}
              </span>
            {/if}
            <span
              class="project-row-path" {@attach truncationTooltip({
                tooltip: project
              })}>{project}</span>
            <LastAccessed now={openedAt} path={project} />
          </span>
        </span>
        {#if isCurrentProject}
          <span class="project-row-check" aria-hidden="true"><Icon name="check" size={15} /></span>
        {/if}
      </button>
    {/snippet}

    <div bind:this={elSwitchList} class="switch-list">
      {#if pinnedShown.length > 0}
        <div class="list-head"><span>Pinned</span></div>
        {#each pinnedShown as project (project)}
          <!-- view-transition-name lets the scoped view transition morph THIS row (a
               moved/removed row glides) instead of cross-fading the whole list;
               data-pin-id keeps a flat drag-sibling set for the reorder engine. -->
          <div
            style:view-transition-name={rowTransitionName(project)}
            class="project-row"
            data-pin-id={filterQuery === "" ? project : undefined}
          >
            {#if isPinListReorderable}
              <span
                class="grip" {@attach tooltip("Drag to reorder")}
                aria-hidden="true"
                onpointerdown={e => beginReorder({
                  e,
                  itemSelector: "[data-pin-id]",
                  idAttribute: "data-pin-id",
                  axis: Axis.Vertical,
                  threshold: ROW_DRAG_THRESHOLD,
                  onCommit: paths => onreorderpins(paths)
                })}
              ><Icon name="grip" size={14} /></span>
            {/if}
            {@render rowMain(project)}
            {@render rowMenu(project, true)}
          </div>
        {/each}
      {/if}

      {#if recentShown.length > 0}
        <div class="list-head">
          <span>Recent</span>
          <button class="clear" onclick={async () => await animateListChange(onclearrecent)} type="button">
            <Icon name="trash" size={12} /> Clear
          </button>
        </div>
        {#each recentShown as project (project)}
          <div style:view-transition-name={rowTransitionName(project)} class="project-row">
            {@render rowMain(project)}
            {@render rowMenu(project, false)}
          </div>
        {/each}
      {/if}

      {#if isFilterUnmatched}
        <div class="no-results">
          No open projects match. Try <strong>Open a project…</strong> below.
        </div>
      {/if}
    </div>

    <div class="menu-divider"></div>

    <button
      class="action" onclick={() => {
        hide();
        onswitch();
      }} role="menuitem" type="button">
      <span class="lead"><Icon name="swap" /></span>
      <span class="grow">Open a project…</span>
      <span class="subtitle">All projects &amp; clone</span>
    </button>

    <div class="menu-divider"></div>

    <div class="menu-separator">New window</div>
    <button
      class="action"
      onclick={async () => await spawn({ mode: WindowMode.enum.empty })}
      role="menuitem"
      type="button"
    >
      <span class="lead accent"><Icon name="windowPlus" /></span>
      <span class="grow">Empty window</span>
      <span class="kbd">Ctrl ⇧ N</span>
    </button>
    <button
      class="action"
      onclick={async () => await spawn({ mode: WindowMode.enum.temp })}
      role="menuitem"
      type="button"
    >
      <span class="lead tertiary"><Icon name="plus" /></span>
      <span class="grow">Throwaway workspace</span>
    </button>

    <!-- Destructive "Delete directory" confirmation. Nested inside this popover so
         the switcher stays visible (dimmed) behind it; on confirm the parent removes
         the folder and the refreshed list animates the row out. -->
    {#if deleteTarget}
      {@const target = deleteTarget}
      <WorkspaceDeleteDialog
        confirmLabel="Delete directory"
        error={deleteError}
        isBusy={isDeleting}
        isNested
        {labels}
        oncancel={() => {
          if (isDeleting) {
            return;
          }

          deleteTarget = null;
          deleteError = "";
        }}
        onconfirm={async () => {
          isDeleting = true;
          deleteError = "";
          try {
            await animateListChange(() => ondelete(target));
            deleteTarget = null;
          } catch (error) {
            deleteError = typeof error === "string" ? error : "Couldn’t delete that directory";
          } finally {
            isDeleting = false;
          }
        }}
        path={target}
        title="Delete this project directory?"
      />
    {/if}

    {#if relabelTarget}
      <RelabelDialog
        currentLabel={labels[relabelTarget] ?? ""}
        nested
        onclose={() => {
          relabelTarget = null;
        }}
        path={relabelTarget}
      />
    {/if}

    {#if renameFolderTarget}
      <RenameFolderDialog
        nested
        onclose={() => {
          renameFolderTarget = null;
        }}
        onrename={onrenamefolder}
        path={renameFolderTarget}
      />
    {/if}
  </div>
</span>

<style>
  .menu-host {
    display: contents;
  }

  .trigger {
    display: inline-flex;
    flex-shrink: 0;
    gap: 8px;
    align-items: center;
    padding-block: 5px;
    padding-inline: 11px 10px;
    border: none;
    border-radius: var(--radius-full);
    background: transparent;
    color: var(--on-surface);
    white-space: nowrap;
    cursor: pointer;
    transition: background 150ms var(--ease);
    anchor-name: --application-menu-anchor;

    &:hover {
      background: var(--surface-2);
    }

    .stack {
      display: inline-flex;
      flex-direction: column;
      align-items: flex-start;
      line-height: 1.1;
    }

    .eyebrow {
      color: var(--on-surface-variant);
      font-weight: 700;
      font-size: 9px;
      letter-spacing: 0.08em;
      text-transform: uppercase;
    }

    .name {
      display: inline-flex;
      gap: 6px;
      align-items: center;
      font-family: var(--font-monospace);
      font-weight: 700;
      font-size: 14px;
    }

    .caret {
      color: var(--on-surface-variant);
      font-weight: 600;
      font-size: 10px;
      opacity: 70%;
    }
  }

  /* Shell comes from the shared .popover-menu; width, colour and anchor side here. */
  .menu {
    inline-size: 352px;
    max-inline-size: 92vw;
    color: var(--on-surface);
    animation: pop-in 220ms var(--spring);
    position-area: bottom span-right;
  }

  /* A window row: a drag grip (when reorderable) beside its focus button, laid out
     like a pinned .project-row so the shared .grip sits flush to the row's left edge. */
  .open-window-row-item {
    position: relative;
    display: flex;
    gap: 2px;
    align-items: center;
  }

  /* An open-window row: focus another window, or "this window" for the current one. */
  .open-window-row {
    display: flex;
    flex: 1;
    gap: 9px;
    align-items: center;
    min-inline-size: 0;
    padding-block: 7px;
    padding-inline: 10px;
    border: none;
    border-radius: var(--radius-small);
    background: transparent;
    color: var(--on-surface);
    font: inherit;
    text-align: start;
    cursor: pointer;
    transition: color 120ms var(--ease), background 120ms var(--ease);

    &.current {
      cursor: default;
    }

    &:not(.current):hover,
    &:not(.current):focus-visible {
      background: var(--primary-container);
      color: var(--on-primary-container);
      outline: none;
    }

    .open-window-row-name {
      overflow: hidden;
      min-inline-size: 0;
      font-family: var(--font-monospace);
      font-weight: 600;
      font-size: 13px;
      text-overflow: ellipsis;
      white-space: nowrap;
    }

    .open-window-row-spacer {
      flex: 1;
      min-inline-size: 8px;
    }

    .this-window {
      flex: none;
      color: var(--primary);
      font-weight: 700;
      font-size: 9px;
      letter-spacing: 0.06em;
      text-transform: uppercase;
    }

    .open-window-row-focus {
      display: inline-flex;
      flex: none;
      color: var(--on-surface-variant);
    }

    &:hover .open-window-row-focus,
    &:focus-visible .open-window-row-focus {
      color: inherit;
    }
  }

  .search {
    display: flex;
    gap: 8px;
    align-items: center;
    margin-block: 1px 5px;
    margin-inline: 2px;
    padding-block: 6px;
    padding-inline: 10px;
    border: 1px solid var(--outline);
    border-radius: 10px;
    background: var(--surface-1);

    .lead {
      display: inline-flex;
      flex: none;
      color: var(--on-surface-variant);
    }

    input {
      flex: 1;
      min-inline-size: 0;
      border: none;
      background: transparent;
      color: var(--on-surface);
      outline: none;
      font: inherit;
      font-size: 13px;
    }

    .kbd {
      flex: none;
      padding-block: 2px;
      padding-inline: 6px;
      border-radius: 6px;
      background: var(--surface-3);
      color: var(--on-surface-variant);
      font-family: var(--font-monospace);
      font-weight: 700;
      font-size: 9px;
    }
  }

  .switch-list {
    display: flex;
    flex-direction: column;
    gap: 1px;
    overflow-y: auto;
    max-block-size: min(46vh, 320px);
    margin-inline: -1px;
  }

  /* Sticky section label (Pinned / Recent) over the scrolling list. */
  .list-head {
    position: sticky;
    inset-block-start: 0;
    z-index: 2;
    display: flex;
    gap: 8px;
    justify-content: space-between;
    align-items: center;
    padding-block: 7px 3px;
    padding-inline: 10px;
    background: var(--surface-2);
    color: var(--on-surface-variant);
    font-weight: 700;
    font-size: 9px;
    letter-spacing: 0.08em;
    text-transform: uppercase;

    .clear {
      display: inline-flex;
      gap: 4px;
      align-items: center;
      padding-block: 2px;
      padding-inline: 7px;
      border: none;
      border-radius: var(--radius-full);
      background: transparent;
      color: var(--on-surface-variant);
      font: inherit;
      font-weight: 600;
      font-size: 10px;
      letter-spacing: normal;
      text-transform: none;
      cursor: pointer;
      transition: color 120ms var(--ease), background 120ms var(--ease);

      &:hover {
        background: var(--critical-wash);
        color: var(--critical);
      }
    }
  }

  /* The recent list is unbounded, so a row scrolled out of the list skips layout
     and paint entirely; `auto` remembers each row's real height once rendered, and
     the fallback is one row's measured height, so the scrollbar stays true. */
  .project-row {
    contain-intrinsic-block-size: auto 41.5px;
    position: relative;
    display: flex;
    content-visibility: auto;
    gap: 2px;
    align-items: center;
  }

  /* Drag handle for a pinned row - muted, brightens on hover; touch-action:none so
     a touch-drag grabs the handle rather than scrolling the list. */
  .grip {
    display: inline-flex;
    flex: none;
    align-items: center;
    align-self: stretch;
    padding-inline: 1px;
    color: var(--on-surface-variant);
    opacity: 55%;
    cursor: grab;
    touch-action: none;
    transition: color 120ms var(--ease), opacity 120ms var(--ease);

    &:hover {
      color: var(--on-surface);
      opacity: 100%;
    }
  }

  .project-row-main {
    display: flex;
    flex: 1;
    gap: 9px;
    align-items: center;
    min-inline-size: 0;
    padding-block: 7px;
    padding-inline: 8px;
    border: none;
    border-radius: var(--radius-small);
    background: transparent;
    color: var(--on-surface);
    font: inherit;
    text-align: start;
    cursor: pointer;
    transition: color 120ms var(--ease), background 120ms var(--ease);

    /* The current project keeps a primary rail on its left edge. */
    &.current {
      box-shadow: inset 3px 0 0 0 var(--primary);
    }

    &:hover,
    &:focus-visible {
      /* The kind icon falls back to the muted ink when a project has no
         detected language (a plain folder), and that ink was picked against the
         row's resting surface - on this fill it is dim at best, and invisible
         where the container is dark. Re-point the token it already reads rather
         than overriding its colour, so a real brand mark (Rust's orange, Go's
         cyan) still wears its own. */
      --on-surface-variant: var(--on-primary-container);

      background: var(--primary-container);
      color: var(--on-primary-container);
      outline: none;
    }
  }

  .project-row-body {
    display: flex;
    flex: 1;
    flex-direction: column;
    min-inline-size: 0;
    line-height: 1.25;
  }

  .project-row-name-line {
    display: flex;
    gap: 6px;
    align-items: center;
    min-inline-size: 0;

    .project-row-name {
      overflow: hidden;
      font-family: var(--font-monospace);
      font-weight: 600;
      font-size: 13px;
      text-overflow: ellipsis;
      white-space: nowrap;
    }
  }

  .project-row-metadata {
    --last-accessed-font-size: 9px;

    display: flex;
    gap: 7px;
    align-items: center;
    min-inline-size: 0;

    .branch {
      display: inline-flex;
      flex: none;
      gap: 4px;
      align-items: center;
      color: var(--on-surface-variant);
      font-family: var(--font-monospace);
      font-weight: 600;
      font-size: 9px;

      /* The branch glyph leads the branch name, tinted tertiary like the design's
         "vc" mark - it stays that colour on row hover (as the dot did). */
      .branch-icon {
        display: inline-flex;
        flex: none;
        color: var(--tertiary);
      }
    }

    .project-row-path {
      overflow: hidden;
      min-inline-size: 0;
      color: var(--on-surface-variant);
      font-family: var(--font-monospace);
      font-size: 9px;
      text-overflow: ellipsis;
      white-space: nowrap;
    }
  }

  .project-row-main:hover .project-row-metadata,
  .project-row-main:focus-visible .project-row-metadata,
  .project-row-main:hover .project-row-metadata .branch,
  .project-row-main:focus-visible .project-row-metadata .branch {
    --last-accessed-color: currentColor;

    color: inherit;
  }

  .project-row-check {
    display: inline-flex;
    flex: none;
    color: var(--primary);
  }

  .project-row-main:hover .project-row-check,
  .project-row-main:focus-visible .project-row-check {
    color: inherit;
  }

  /* Row kebab - the ⋮ button opening the per-row options popover. */
  .project-row-kebab {
    display: inline-flex;
    flex: none;
    justify-content: center;
    align-items: center;
    block-size: 28px;
    inline-size: 28px;
    padding: 0;
    border: none;
    border-radius: var(--radius-small);
    background: transparent;
    color: var(--on-surface-variant);
    opacity: 55%;
    cursor: pointer;
    transition: color 120ms var(--ease), background 120ms var(--ease), opacity 120ms var(--ease);

    &:hover {
      background: var(--surface-3);
      color: var(--on-surface);
      opacity: 100%;
    }
  }

  /* Reveal the kebab on row hover/focus (or while its own menu is open). */
  .project-row:hover .project-row-kebab,
  .project-row:focus-within .project-row-kebab {
    opacity: 100%;
  }

  /* Per-row options popover - Pin/Unpin, Remove, Delete. Width is capped so the
     Delete item's path ellipsizes (.menu-item-subtitle) instead of ballooning the menu wide
     enough to spill past the panel over the terminal. */
  .row-menu {
    min-inline-size: 210px;
    max-inline-size: 260px;
    position-area: bottom span-left;

    .menu-item {
      display: flex;
      gap: 9px;
      align-items: center;
      inline-size: 100%;
      padding-block: 8px;
      padding-inline: 9px;
      border: none;
      border-radius: var(--radius-small);
      background: transparent;
      color: var(--on-surface);
      font: inherit;
      font-weight: 500;
      font-size: 13px;
      text-align: start;
      cursor: pointer;
      transition: color 120ms var(--ease), background 120ms var(--ease);

      &:hover,
      &:focus-visible {
        background: var(--primary-container);
        color: var(--on-primary-container);
        outline: none;
      }

      .menu-item-icon {
        display: inline-flex;
        flex: none;
        color: var(--on-surface-variant);
      }

      &:hover .menu-item-icon,
      &:focus-visible .menu-item-icon {
        color: inherit;
      }

      .menu-item-body {
        display: flex;
        flex-direction: column;
        gap: 1px;
        min-inline-size: 0;
      }

      .menu-item-subtitle {
        overflow: hidden;
        color: var(--on-surface-variant);
        font-family: var(--font-monospace);
        font-size: 10px;
        text-overflow: ellipsis;
        white-space: nowrap;
      }
    }

    /* Delete reads critical-red at rest so it's legible as dangerous before hover;
       its wash on hover stays critical rather than the primary fill. */
    .menu-item.critical {
      color: var(--critical);

      .menu-item-icon {
        color: var(--critical);
      }

      &:hover,
      &:focus-visible {
        background: var(--critical-wash);
        color: var(--critical);
      }

      &:hover .menu-item-icon,
      &:focus-visible .menu-item-icon,
      &:hover .menu-item-subtitle,
      &:focus-visible .menu-item-subtitle {
        color: var(--critical);
      }
    }
  }

  .no-results {
    padding-block: 14px;
    padding-inline: 10px;
    color: var(--on-surface-variant);
    font-size: 12px;
    text-align: center;

    strong {
      color: var(--on-surface);
    }
  }

  .action {
    display: flex;
    gap: 10px;
    align-items: center;
    inline-size: 100%;
    padding-block: 8px;
    padding-inline: 10px;
    border: none;
    border-radius: var(--radius-small);
    background: transparent;
    color: var(--on-surface);
    font: inherit;
    font-weight: 600;
    font-size: 13px;
    text-align: start;
    cursor: pointer;
    transition: color 120ms var(--ease), background 120ms var(--ease);

    &:hover,
    &:focus-visible {
      background: var(--primary-container);
      color: var(--on-primary-container);
      outline: none;
    }

    .grow {
      flex: 1;
      min-inline-size: 0;
    }

    .lead {
      display: inline-flex;
      flex: none;
      color: var(--on-surface-variant);

      &.accent {
        color: var(--primary);
      }

      &.tertiary {
        color: var(--tertiary);
      }
    }

    .subtitle {
      flex: none;
      color: var(--on-surface-variant);
      font-weight: 600;
      font-size: 10px;
    }

    .kbd {
      flex: none;
      padding-block: 2px;
      padding-inline: 6px;
      border-radius: 6px;
      background: var(--surface-3);
      color: var(--on-surface-variant);
      font-family: var(--font-monospace);
      font-size: 10px;
    }

    &:hover .lead,
    &:focus-visible .lead,
    &:hover .subtitle,
    &:focus-visible .subtitle,
    &:hover .kbd,
    &:focus-visible .kbd {
      color: inherit;
    }
  }
</style>
