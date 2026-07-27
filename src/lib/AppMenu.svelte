<script lang="ts">
  import { vcs, windows, workspace } from "@/lib/bridge";
  import ConfirmDialog from "@/lib/ConfirmDialog.svelte";
  import { Axis, beginReorder } from "@/lib/drag-reorder";
  import Icon from "@/lib/Icon.svelte";
  import Logo from "@/lib/Logo.svelte";
  import {
    childPath,
    displayName,
    isTemporaryWorkspace,
    normalizePath,
    shortDisplayName
  } from "@/lib/paths";
  import ProjectKindIcon from "@/lib/ProjectKindIcon.svelte";
  import { openRepositoryOnModifiedClick } from "@/lib/repository-links";
  import { truncationTooltip } from "@/lib/truncation-tooltip";
  import { AddRootStatus, WindowMode } from "@/lib/types";
  import type { AddRootOutcome, WindowInfo } from "@/lib/types";
  import { nameError, parseInput, ProjectName } from "@/lib/validate";
  import type { UnlistenFn } from "@tauri-apps/api/event";
  import { open as openDialog } from "@tauri-apps/plugin-dialog";
  import { onDestroy, onMount, tick } from "svelte";

  // The project switcher that leads the top bar. It lists every open PADE window
  // (jump between them, or cycle with Ctrl+Alt+[ / ]), then is a fast way to
  // switch THIS window to another project: type to filter, or click a pinned/recent
  // row. New windows and the full picker sit below. In-window switches funnel
  // through `onopen`; pin/remove/reorder persist via parent callbacks and the
  // shared settings authority; window focus + the list use the bridge directly.
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
    onreorderpins,
    onsavetemp,
    onaddroot
  }: {
    path: string;
    label: string;
    isTemp: boolean;
    /** The saved project roots — destinations the temp workspace can be saved into. */
    roots: string[];
    recentProjects: string[];
    pinnedProjects: string[];
    labels: Record<string, string>;
    /** Switch this window to `path` (in place). */
    onopen: (path: string) => void;
    /** Open the full picker (browse every root, clone, open a folder). */
    onswitch: () => void;
    /** Pin or unpin a project — persisted by the parent into shared settings.
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
    /** Delete a project's directory from disk — performs the removal and resolves
     *  (or throws its message). The switcher owns the confirmation UI, so it stays
     *  open behind the prompt and animates the row out when this resolves. */
    ondelete: (path: string) => Promise<void>;
    /** Persist a drag-reordered pin order. */
    onreorderpins: (paths: string[]) => void;
    /** Save the current temp workspace as a real project: rename it into `root`
     *  (one of the saved roots) under `name`. Resolves once relocated. */
    onsavetemp: (target: {
      name: string;
      root: string;
    }) => Promise<void>;
    /** Add an existing folder as a project root — the no-roots save path.
     *  Persisted by the parent into shared settings. */
    onaddroot: (path: string) => Promise<AddRootOutcome>;
  } = $props();

  let filter = $state("");
  // Branches and the open-window list are fetched when the menu opens. Language
  // icons load independently, only as their rows become visible.
  let branches = $state<Record<string, string>>({});
  let windowRows = $state<WindowInfo[]>([]);
  // Whether the popover is open — the moment a row first has a real width, so the
  // per-row truncation tooltips (re)measure then rather than while hidden.
  let menuOpen = $state(false);
  // The rows container, so list mutations animate through a view transition
  // scoped to *just* this element (never the document — that would snapshot the
  // live-repainting terminal and ghost it).
  let switchListElement = $state<HTMLElement | null>(null);
  let windowListElement = $state<HTMLElement | null>(null);

  // The "Delete directory" confirmation lives inside this menu (a nested popover)
  // so the switcher stays visible behind it; the parent only performs the removal.
  let deletePending = $state<string | null>(null);
  let deleteBusy = $state(false);
  let deleteError = $state("");

  // ── Save-this-workspace (temp → real project) ───────────────────────────────
  // Name the throwaway workspace and pick which saved root receives it. One root
  // saves straight into it; several list a Save button each; none offers to
  // create a root first (native folder picker) and then saves into it.
  // Prefilled as the menu opens with the friendly label when one exists
  // (auto-name), never the raw temp-<stamp> folder name; anything typed sticks.
  let saveName = $state("");
  function prefillSaveName() {
    const friendlyLabel = parseInput({
      schema: ProjectName,
      raw: label
    });
    const labelIsPrefillable = saveName === "" && friendlyLabel !== null && !/^temp-\d+$/.test(friendlyLabel);
    if (labelIsPrefillable) {
      saveName = friendlyLabel;
    }
  }
  let saveBusy = $state(false);
  let saveFailure = $state("");
  const saveNameIssue = $derived(nameError(saveName));
  const savableName = $derived(
    parseInput({
      schema: ProjectName,
      raw: saveName
    })
  );

  // Live disk check on each save target — the same collision guard the project
  // picker's new-project field uses (QuickStartSection). `workspace_rename` does
  // an `fs::rename`, which on Windows fails with a raw "os error 32/183" if the
  // target already exists; probe `<root>/<name>` up front so a colliding name
  // disables that root's Save with a plain reason instead of a cryptic failure.
  // Each probe is tagged with the name it described, so an out-of-order reply
  // never gates the current text.
  type SaveProbe = {
    name: string;
    root: string;
    exists: boolean;
  };
  let saveProbes = $state<SaveProbe[]>([]);
  $effect(() => {
    const name = savableName;
    if (name === null || roots.length === 0) {
      saveProbes = [];
      return;
    }

    async function probeRoots(targetName: string) {
      const probed = await Promise.all(
        roots.map(async root => {
          try {
            const probe = await workspace.probePath(
              childPath({
                parent: root,
                name: targetName
              })
            );
            return {
              name: targetName,
              root,
              exists: probe.isDir || probe.isFile
            };
          } catch {
            // An unreachable probe leaves the plain save path — the backend
            // still validates the final rename.
            return {
              name: targetName,
              root,
              exists: false
            };
          }
        })
      );
      // Only believe an answer that describes the name still in the field.
      if (targetName === savableName) {
        saveProbes = probed;
      }
    }
    probeRoots(name);
  });
  function saveCollides(root: string): boolean {
    return saveProbes.some(probe => probe.root === root && probe.name === savableName && probe.exists);
  }

  // Rename into `root`; the menu closes on success (the window relocates under
  // the saved path). A failure stays visible inside the card.
  async function saveTempInto(root: string) {
    if (!savableName || saveBusy) {
      return;
    }

    saveBusy = true;
    saveFailure = "";
    try {
      await onsavetemp({
        name: savableName,
        root
      });
      hide();
    } catch (error) {
      saveFailure = String(error);
    } finally {
      saveBusy = false;
    }
  }

  // Enter in the name field saves into the PRIMARY root (roots[0]) — the same
  // default workspace_rename uses when no root is chosen — or opens the folder
  // picker when there are no roots yet. Respects the same gates as the primary
  // root's button (blank/busy/collision), so Enter can't save a name that button
  // shows disabled.
  async function saveViaEnter() {
    if (!savableName || saveBusy) {
      return;
    }

    if (roots.length === 0) {
      await createRootAndSave();
      return;
    }

    const primaryRoot = roots[0];
    if (saveCollides(primaryRoot)) {
      return;
    }

    await saveTempInto(primaryRoot);
  }

  // The no-roots path: pick an existing folder as the first root, then save
  // into it. The dialog always returns an existing directory, so any non-`added`
  // outcome is a real failure worth showing.
  async function createRootAndSave() {
    const picked = await openDialog({
      directory: true,
      multiple: false
    });
    if (typeof picked !== "string") {
      return;
    }

    const outcome = await onaddroot(picked);
    if (outcome.status !== AddRootStatus.enum.added) {
      saveFailure = "That folder can't be used as a root.";
      return;
    }

    await saveTempInto(picked);
  }

  const pinnedSet = $derived(new Set(pinnedProjects.map(normalizePath)));
  // Recents minus anything already pinned, so a project shows in one section only.
  const recentsOnly = $derived(recentProjects.filter(project => !pinnedSet.has(normalizePath(project))));

  function matchesFilter(project: string): boolean {
    const query = filter.trim().toLowerCase();
    if (!query) {
      return true;
    }

    return (
      displayName(project, labels).toLowerCase().includes(query) ||
      project.toLowerCase().includes(query)
    );
  }

  const pinnedShown = $derived(pinnedProjects.filter(matchesFilter));
  const recentShown = $derived(recentsOnly.filter(matchesFilter));
  const noResults = $derived(
    filter.trim().length > 0 && pinnedShown.length === 0 && recentShown.length === 0
  );
  // Pins reorder only when there's more than one and no filter narrows the set —
  // the drag engine commits the visible rows, so a filtered subset would drop the
  // hidden pins from the saved order.
  const pinsReorderable = $derived(filter.trim() === "" && pinnedProjects.length > 1);
  // A lone window can't be reordered — mirror the pins guard so the grip only
  // appears when there's more than one open window to drag among.
  const windowsReorderable = $derived(windowRows.length > 1);

  function isCurrent(project: string): boolean {
    return normalizePath(project) === normalizePath(path);
  }
  // Stable, valid popover id/anchor per row kebab (sanitised from the path).
  function rowMenuId(project: string): string {
    return `switcher-${project.replaceAll(/[^a-zA-Z0-9]/g, "-")}`;
  }
  // Stable, unique view-transition-name per row, so the scoped view transition
  // tracks each row across a list change and morphs it (a moved row glides to its
  // new slot) instead of cross-fading the whole list. The same project keeps the
  // same name across sections, so pinning it glides from Recent to Pinned.
  function rowTransitionName(project: string): string {
    return `row-${project.replaceAll(/[^a-zA-Z0-9]/g, "-")}`;
  }
  // A separate namespace from `rowTransitionName`, keyed by the window's stable
  // session label: a window and a pinned row can share a project path, and two
  // elements can't carry the same view-transition-name in one transition.
  function windowRowTransitionName(label: string): string {
    return `window-row-${label.replaceAll(/[^a-zA-Z0-9]/g, "-")}`;
  }

  // Fetch open windows and branches. Language kinds are deliberately absent:
  // ProjectKindIcon batches only visible rows and never waits on Git processes.
  async function loadBranches(windowPaths: string[]) {
    const paths = [
      ...new Set([path, ...pinnedProjects, ...recentProjects, ...windowPaths])
    ].filter(Boolean);
    if (paths.length === 0) {
      return;
    }

    try {
      branches = await vcs.branchOf(paths);
    } catch {
    // Preserve last-known branches; icon detection remains independent.
    }
  }

  async function loadMeta() {
    let openWindows: WindowInfo[];
    try {
      openWindows = await windows.list();
    } catch {
      return;
    }
    windowRows = openWindows;
    await loadBranches(openWindows.map(window => window.path));
  }

  // Live cross-window sync: another window changed the open-windows set/order (a
  // drag-reorder landed there), so reflect it here with the same scoped morph the
  // pinned/recent list uses. Only the row ORDER is animated — the branch fetch is
  // slow and would freeze the snapshotted menu, so it runs after the transition.
  async function onWindowsChanged() {
    if (!menuOpen) {
      return;
    }

    let openWindows: WindowInfo[];
    try {
      openWindows = await windows.list();
    } catch {
      return;
    }

    await animateWindowListChange(async () => {
      windowRows = openWindows;
      await tick();
    });
    await loadBranches(openWindows.map(window => window.path));
  }

  // Persist a drag-reordered window order to the one backend source (which drives
  // both this list and the Ctrl+Alt+[ / ] cycle), reflect it at once, then re-fetch
  // so the rows match backend truth. The drag engine hands us the new label order.
  async function reorderWindows(labels: string[]) {
    const byLabel = new Map(windowRows.map(row => [row.label, row]));
    windowRows = labels.flatMap(labelId => {
      const row = byLabel.get(labelId);
      return row ? [row] : [];
    });
    await windows.reorder(labels);
    await loadMeta();
  }

  function hide() {
    const menu = document.getElementById("application-menu");
    if (menu?.matches(":popover-open")) {
      menu.hidePopover();
    }
  }

  // Jump this window to a project (or, with Ctrl/Cmd/Shift held, open it in a new
  // window instead of switching this one).
  function pick(project: string, e: MouseEvent) {
    hide();

    if (isCurrent(project)) {
      return;
    }

    const opensNewWindow = e.ctrlKey || e.metaKey || e.shiftKey;
    if (opensNewWindow) {
      windows.create({
        mode: WindowMode.enum.open,
        path: project
      });
      return;
    }

    onopen(project);
  }

  // Focus the filter as the menu opens: it's ready to type, and — crucially —
  // the trigger no longer keeps focus, so a later keypress (holding Shift for a
  // shift-click) can't flip `:focus-visible` on and paint a stray ring on it. The
  // rAF waits for the popover's top layer to lay out before the field is focusable.
  function focusFilter() {
    requestAnimationFrame(() => document.getElementById("application-menu-query")?.focus());
  }

  // Spawn a fresh window and dismiss the menu so it doesn't linger over the new
  // one. `path` is optional — omitted for empty/temp modes.
  async function spawn(args: {
    mode: WindowMode;
    path?: string;
  }) {
    await windows.create(args);
    hide();
  }

  // Run a list mutation (pin/unpin/remove/delete/clear — each an async settings
  // update that reshapes the rows) inside a view transition scoped to *just* the
  // rows container, so a row morphs — one moved between sections glides to its new
  // place — instead of snapping. Scoped to this element, never `document`, so it
  // never snapshots the live-repainting terminal (which would ghost). Reduced
  // motion, or an engine without the scoped API, runs the mutation directly.
  type ScopedTransitionList = HTMLElement & {
    startViewTransition?: (callback: () => Promise<void>) => { updateCallbackDone: Promise<void> };
  };

  async function runListTransition(list: ScopedTransitionList | null, mutate: () => Promise<void>) {
    if (matchMedia("(prefers-reduced-motion: reduce)").matches || !list?.startViewTransition) {
      await mutate();
      return;
    }

    // Resolve once the DOM has updated (so a caller like the delete flow can close
    // its dialog then) — the transition itself keeps animating after that.
    const transition = list.startViewTransition(async () => {
      await mutate();
      await tick();
    });
    await transition.updateCallbackDone;
  }

  async function animateListChange(mutate: () => Promise<void>) {
    await runListTransition(switchListElement, mutate);
  }

  // The Open-windows list gets the same scoped morph as the pinned/recent list,
  // so a reorder animates in place. Its rows live outside `switchListElement`, so
  // the transition is scoped to their own container.
  async function animateWindowListChange(mutate: () => Promise<void>) {
    await runListTransition(windowListElement, mutate);
  }

  // Ctrl P from anywhere opens the switcher and focuses its filter, matching the
  // shortcut the trigger and the search field advertise. It runs in the capture
  // phase (like pane-shortcuts) so it wins over the terminal — xterm would
  // otherwise swallow Ctrl P and send it to the agent — and the browser's print
  // dialog, neither of which sees the chord once it's consumed here.
  function openViaShortcut(e: KeyboardEvent) {
    const isCtrlP =
      (e.ctrlKey || e.metaKey) && !e.shiftKey && !e.altKey && e.key.toLowerCase() === "p";
    if (!isCtrlP) {
      return;
    }

    e.preventDefault();
    e.stopImmediatePropagation();
    const menu = document.getElementById("application-menu");
    if (menu && !menu.matches(":popover-open")) {
      menu.showPopover();
    }

    focusFilter();
  }

  onMount(() => {
    addEventListener("keydown", openViaShortcut, { capture: true });
    return () => removeEventListener("keydown", openViaShortcut, { capture: true });
  });

  // Keep the branch chips honest while the switcher is on screen: a branch
  // switch or git init in any listed project re-fetches the row metadata. A
  // closed menu skips the fetch — it reloads on every open anyway.
  let unlistenGitState: UnlistenFn | undefined;
  onMount(async () => {
    unlistenGitState = await vcs.onStateChanged(() => {
      if (menuOpen) {
        loadMeta();
      }
    });
  });
  onDestroy(() => unlistenGitState?.());

  // Reflect open-windows changes made in OTHER windows live (their reorder lands
  // → this switcher morphs to match), so two open menus never disagree.
  let unlistenWindowsChanged: UnlistenFn | undefined;
  onMount(async () => {
    unlistenWindowsChanged = await windows.onChanged(onWindowsChanged);
  });
  onDestroy(() => unlistenWindowsChanged?.());
</script>

<!-- Close the menu on Escape from ANYWHERE, including while the terminal holds
     focus: xterm's textarea preventDefaults Escape to forward it to the agent,
     which swallows the popover's native close-signal. A capture-phase window
     handler runs before xterm, so it wins while the menu is open. -->
<svelte:window
  onkeydowncapture={e => {
    if (menuOpen && e.key === "Escape") {
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
    aria-label={`Switch project · Ctrl P · Ctrl-click opens repository — ${label}`}
    data-tooltip="Switch project · Ctrl-click opens repository"
    popovertarget="application-menu"
  >
    <Logo size={18} />
    <span class="stack">
      <span class="eyebrow">Project</span>
      <span class="name">
        {label}
        {#if isTemp}
          <span class="temporary">temp</span>
        {/if}
      </span>
    </span>
    <span class="caret" aria-hidden="true">▾</span>
  </button>

  <div
    id="application-menu"
    style:position-anchor="--application-menu-anchor"
    class="menu popover-menu"
    ontoggle={async e => {
      menuOpen = (e as ToggleEvent).newState === "open";

      if (menuOpen) {
        prefillSaveName();
        focusFilter();
        await loadMeta();
      }
    }}
    popover
    role="menu"
  >
    <!-- Save the throwaway workspace as a real project: name + destination root.
         Design's "Save this workspace" card, shown only for a temp project. -->
    {#if isTemp}
      <div class="save-card">
        <div class="save-head">
          <span class="temporary">temp</span>
          <span class="save-title">Save this workspace</span>
        </div>
        <p class="save-hint">Name it and pick a root to keep it beyond this session.</p>
        <form
          class="save-form" onsubmit={async e => {
            e.preventDefault();
            await saveViaEnter();
          }}>
          <input
            class="save-name"
            aria-label="Project name"
            autocomplete="off"
            placeholder="project-name"
            spellcheck="false"
            bind:value={saveName}
          />
          {#if saveNameIssue}
            <output class="save-issue">{saveNameIssue}</output>
          {/if}
          {#if saveFailure}
            <output class="save-issue">{saveFailure}</output>
          {/if}
          {#if roots.length === 0}
            <button
              class="save-root"
              disabled={!savableName || saveBusy}
              onclick={async () => await createRootAndSave()}
              type="button"
            >
              <span class="save-root-icon" aria-hidden="true"><Icon name="folderPlus" size={15} /></span>
              <span class="save-root-path">Choose a folder for your projects…</span>
              <span class="save-go">Create root & save →</span>
            </button>
          {:else}
            <div class="save-roots">
              {#each roots as root (root)}
                {@const collides = saveCollides(root)}
                <button
                  class="save-root"
                  class:save-root-collides={collides}
                  disabled={!savableName || saveBusy || collides}
                  onclick={async () => await saveTempInto(root)}
                  type="button"
                >
                  <span class="save-root-icon" aria-hidden="true"><Icon name="folder" size={15} /></span>
                  <span class="save-root-path">{root}</span>
                  <span class="save-go">
                    {#if collides}
                      Name taken
                    {:else}
                      Save →
                    {/if}
                  </span>
                </button>
              {/each}
            </div>
          {/if}
        </form>
      </div>
      <div class="separator"></div>
    {/if}

    <!-- Open PADE windows — in creation order, which is also the cycle order for
       Ctrl+Alt+[ / ]. Click a non-current one to focus its window. -->
    {#if windowRows.length > 0}
      <div class="eyebrow section">Open windows</div>
      <!-- Own container so the scoped view transition (animateWindowListChange)
           morphs just these rows when the order changes — locally or when another
           window's reorder broadcasts. -->
      <div bind:this={windowListElement} class="open-window-list">
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
            {#if windowsReorderable}
              <span
                class="grip"
                aria-hidden="true"
                data-tooltip="Drag to reorder"
                onpointerdown={e => beginReorder({
                  e,
                  itemSelector: "[data-window-id]",
                  idAttribute: "data-window-id",
                  axis: Axis.Vertical,
                  threshold: 4,
                  onCommit: labelOrder => reorderWindows(labelOrder)
                })}
              ><Icon name="grip" size={14} /></span>
            {/if}
            <button
              class="open-window-row"
              class:current={windowRow.isCurrent}
              onclick={() => {
                if (!windowRow.isCurrent) {
                  hide();
                  windows.focus(windowRow.label);
                }
              }}
              role="menuitem"
              type="button"
            >
              <ProjectKindIcon path={windowRow.path} />
              <span class="open-window-row-name">{shortDisplayName(windowRow.path, labels)}</span>
              {#if isTemporaryWorkspace(windowRow.path)}
                <span class="temporary">temp</span>
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
      <div class="separator"></div>
    {/if}

    <!-- Filter / quick-switch -->
    <label class="search">
      <span class="lead" aria-hidden="true"><Icon name="search" size={15} /></span>
      <input
        id="application-menu-query"
        aria-label="Switch project by name or path"
        autocomplete="off"
        onkeydown={e => {
          if (e.key !== "Enter") {
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
    {#snippet rowMenu(project: string, pinned: boolean, menuId: string)}
      <button
        class="project-row-kebab"
        aria-haspopup="menu"
        aria-label={`Options for ${displayName(project, labels)}`}
        popovertarget={menuId}
        type="button"
      ><Icon name="more" size={16} /></button>
      <div id={menuId} class="row-menu popover-menu" popover role="menu">
        <button
          class="menu-item" onclick={() => animateListChange(() => ontogglepin({
            path: project,
            pinned: !pinned
          }))}
          popovertarget={menuId}
          popovertargetaction="hide"
          role="menuitem"
          type="button">
          <span class="menu-item-icon"><Icon name="star" size={15} /></span>
          <span>{#if pinned}
            Unpin from top{:else}Pin to top{/if}</span>
        </button>
        <button
          class="menu-item" onclick={() => animateListChange(() => onremoverecent(project))}
          popovertarget={menuId}
          popovertargetaction="hide"
          role="menuitem"
          type="button">
          <span class="menu-item-icon"><Icon name="close" size={15} /></span>
          <span>Remove from list</span>
        </button>
        <div class="separator"></div>
        <button
          class="menu-item critical" onclick={() => {
            deleteError = "";
            deletePending = project;
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
      {@const current = isCurrent(project)}
      <button
        class="project-row-main"
        class:current
        aria-checked={current}
        onclick={e => pick(project, e)}
        role="menuitemradio"
        type="button"
      >
        <ProjectKindIcon path={project} />
        <span class="project-row-body">
          <span class="project-row-name-line">
            <span class="project-row-name">{displayName(project, labels)}</span>
            {#if isTemporaryWorkspace(project)}
              <span class="temporary">temp</span>
            {/if}
          </span>
          <span class="project-row-metadata">
            {#if branches[project]}
              <span class="branch">
                <span class="branch-icon" aria-hidden="true"><Icon name="branch" size={11} /></span>
                {branches[project]}
              </span>
            {/if}
            <span
              class="project-row-path" {@attach truncationTooltip({
                tooltip: project
              })}>{project}</span>
          </span>
        </span>
        {#if current}
          <span class="project-row-check" aria-hidden="true"><Icon name="check" size={15} /></span>
        {/if}
      </button>
    {/snippet}

    <div bind:this={switchListElement} class="switch-list">
      {#if pinnedShown.length > 0}
        <div class="list-head"><span>Pinned</span></div>
        {#each pinnedShown as project (project)}
          {@const menuId = rowMenuId(project)}
          <!-- view-transition-name lets the scoped view transition morph THIS row (a
               moved/removed row glides) instead of cross-fading the whole list;
               data-pin-id keeps a flat drag-sibling set for the reorder engine. -->
          <div
            style:view-transition-name={rowTransitionName(project)}
            class="project-row"
            data-pin-id={filter.trim() === "" ? project : undefined}
          >
            {#if pinsReorderable}
              <span
                class="grip"
                aria-hidden="true"
                data-tooltip="Drag to reorder"
                onpointerdown={e => beginReorder({
                  e,
                  itemSelector: "[data-pin-id]",
                  idAttribute: "data-pin-id",
                  axis: Axis.Vertical,
                  threshold: 4,
                  onCommit: paths => onreorderpins(paths)
                })}
              ><Icon name="grip" size={14} /></span>
            {/if}
            {@render rowMain(project)}
            {@render rowMenu(project, true, menuId)}
          </div>
        {/each}
      {/if}

      {#if recentShown.length > 0}
        <div class="list-head">
          <span>Recent</span>
          <button class="clear" onclick={() => animateListChange(() => onclearrecent())} type="button">
            <Icon name="trash" size={12} /> Clear
          </button>
        </div>
        {#each recentShown as project (project)}
          {@const menuId = rowMenuId(project)}
          <div style:view-transition-name={rowTransitionName(project)} class="project-row">
            {@render rowMain(project)}
            {@render rowMenu(project, false, menuId)}
          </div>
        {/each}
      {/if}

      {#if noResults}
        <div class="no-results">
          No open projects match. Try <strong>Open a project…</strong> below.
        </div>
      {/if}
    </div>

    <div class="separator"></div>

    <button
      class="action" onclick={() => {
        hide();
        onswitch();
      }} role="menuitem" type="button">
      <span class="lead"><Icon name="swap" /></span>
      <span class="grow">Open a project…</span>
      <span class="subtitle">All projects &amp; clone</span>
    </button>

    <div class="separator"></div>

    <div class="eyebrow section">New window</div>
    <button class="action" onclick={() => spawn({ mode: WindowMode.enum.empty })} role="menuitem" type="button">
      <span class="lead accent"><Icon name="windowPlus" /></span>
      <span class="grow">Empty window</span>
      <span class="kbd">Ctrl ⇧ N</span>
    </button>
    <button class="action" onclick={() => spawn({ mode: WindowMode.enum.temp })} role="menuitem" type="button">
      <span class="lead tertiary"><Icon name="plus" /></span>
      <span class="grow">Throwaway workspace</span>
    </button>

    <!-- Destructive "Delete directory" confirmation. Nested inside this popover so
         the switcher stays visible (dimmed) behind it; on confirm the parent removes
         the folder and the refreshed list animates the row out. -->
    {#if deletePending}
      {@const target = deletePending}
      <ConfirmDialog
        busy={deleteBusy}
        busyLabel="Deleting…"
        confirmLabel="Delete directory"
        danger
        error={deleteError}
        icon="trash"
        nested
        oncancel={() => {
          if (deleteBusy) {
            return;
          }

          deletePending = null;
          deleteError = "";
        }}
        onconfirm={async () => {
          deleteBusy = true;
          deleteError = "";
          try {
            await animateListChange(() => ondelete(target));
            deletePending = null;
          } catch (error) {
            deleteError = typeof error === "string" ? error : "Couldn’t delete that directory.";
          } finally {
            deleteBusy = false;
          }
        }}
        title="Delete this project directory?"
      >
        <div class="directory-delete-body">
          <p>The folder and everything inside it is removed from disk. This can’t be undone.</p>
          <p class="target">
            <span class="target-name">{displayName(target, labels)}</span>
            <code>{target}</code>
          </p>
        </div>
      </ConfirmDialog>
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
    border-radius: 999px;
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

  /* A small temp pill, reused in the trigger, the window rows, and project rows. */
  .temporary {
    flex: none;
    padding-block: 1px;
    padding-inline: 6px;
    border-radius: 999px;
    background: var(--surface-3);
    color: var(--on-surface-variant);
    font-family: var(--font-ui);
    font-weight: 700;
    font-size: 9px;
    letter-spacing: 0.06em;
    text-transform: uppercase;
  }

  /* Shell comes from the shared .popover-menu; width, colour and anchor side here. */

  /* "Save this workspace" — the temp-promotion card leading the menu. */
  .save-card {
    display: flex;
    flex-direction: column;
    gap: 8px;
    margin-block: 1px 6px;
    margin-inline: 2px;
    padding: 10px;
    border: 1px solid var(--outline);
    border-radius: var(--radius-medium);
    background: var(--surface-1);

    /* The <form> exists only to make Enter submit; it must not become a flex item
       or it would swallow the card's column gap around the field and roots. */
    .save-form {
      display: contents;
    }

    .save-head {
      display: flex;
      gap: 6px;
      align-items: center;
    }

    .save-title {
      font-weight: 700;
      font-size: 12px;
    }

    .save-hint {
      margin: 0;
      color: var(--on-surface-variant);
      font-size: 10px;
      line-height: 1.4;
    }

    .save-name {
      inline-size: 100%;
      padding-block: 7px;
      padding-inline: 8px;
      border: 1px solid var(--outline);
      border-radius: var(--radius-small);
      background: var(--surface-2);
      color: var(--on-surface);
      outline: none;
      font-family: var(--font-monospace);
      font-weight: 600;
      font-size: 12px;

      &:focus-visible {
        border-color: var(--primary);
      }
    }

    .save-issue {
      color: var(--critical);
      font-weight: 600;
      font-size: 10.5px;
      line-height: 1.4;
    }

    .save-roots {
      display: flex;
      flex-direction: column;
      gap: 3px;
    }

    .save-root {
      display: flex;
      gap: 8px;
      align-items: center;
      inline-size: 100%;
      padding-block: 7px;
      padding-inline: 8px;
      border-radius: var(--radius-small);
      background: transparent;
      color: var(--on-surface);
      font: inherit;
      text-align: start;
      cursor: pointer;
      transition: color 120ms var(--ease), background 120ms var(--ease);

      &:hover:not(:disabled) {
        background: var(--primary-container);
        color: var(--on-primary-container);
      }

      &:disabled {
        opacity: 55%;
        cursor: default;
      }

      .save-root-icon {
        display: inline-flex;
        flex: none;
        color: var(--on-surface-variant);
      }

      .save-root-path {
        flex: 1;
        overflow: hidden;
        min-inline-size: 0;
        font-family: var(--font-monospace);
        font-weight: 600;
        font-size: 11px;
        text-overflow: ellipsis;
        white-space: nowrap;
      }

      .save-go {
        flex: none;
        color: var(--primary);
        font-weight: 700;
        font-size: 10px;
      }

      &:hover:not(:disabled) .save-go {
        color: var(--on-primary-container);
      }

      &.save-root-collides .save-go {
        color: var(--critical);
      }
    }
  }

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
      border-radius: 999px;
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

  .project-row {
    position: relative;
    display: flex;
    gap: 2px;
    align-items: center;
  }

  /* Drag handle for a pinned row — muted, brightens on hover; touch-action:none so
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
         "vc" mark — it stays that colour on row hover (as the dot did). */
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

  /* Row kebab — the ⋮ button opening the per-row options popover. */
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

  /* Per-row options popover — Pin/Unpin, Remove, Delete. Width is capped so the
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

  .separator {
    block-size: 1px;
    margin-block: 6px;
    margin-inline: 8px;
    background: var(--outline);
  }

  .eyebrow.section {
    padding-block: 6px 4px;
    padding-inline: 10px;
    color: var(--on-surface-variant);
    font-weight: 700;
    font-size: 10px;
    letter-spacing: 0.08em;
    text-transform: uppercase;
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

  /* Body of the "Delete directory" confirmation (chrome is ConfirmDialog's). */
  .directory-delete-body {
    p {
      margin: 0;
    }

    .target {
      display: flex;
      flex-direction: column;
      gap: 2px;
      margin-block-start: 14px;
      padding: 10px 12px;
      border-radius: var(--radius-medium);
      background: var(--surface-2);
    }

    .target-name {
      color: var(--on-surface);
      font-family: var(--font-monospace);
      font-weight: 600;
      font-size: 13px;
    }

    code {
      color: var(--on-surface-variant);
      font-family: var(--font-monospace);
      font-size: 11px;
      overflow-wrap: anywhere;
    }
  }
</style>
