<script lang="ts">
  import { ide, vcs, windows } from "@/lib/bridge";
  import ConfirmDialog from "@/lib/ConfirmDialog.svelte";
  import { Axis, beginReorder } from "@/lib/drag-reorder";
  import Icon from "@/lib/Icon.svelte";
  import type { IconName } from "@/lib/Icon.svelte";
  import { languageIcon } from "@/lib/language-icon";
  import Logo from "@/lib/Logo.svelte";
  import { displayName, isTemporaryWorkspace, normalizePath, shortDisplayName } from "@/lib/paths";
  import { truncationTooltip } from "@/lib/truncation-tooltip";
  import type { WindowInfo } from "@/lib/types";
  import type { UnlistenFn } from "@tauri-apps/api/event";
  import { onDestroy, onMount, tick } from "svelte";

  // The project switcher that leads the top bar. It lists every open PADE window
  // (jump between them, or cycle with Ctrl+Alt+[ / ]), then is a fast way to
  // switch THIS window to another project: type to filter, or click a pinned/recent
  // row. New windows and the full picker sit below. In-window switches funnel
  // through `onopen`; pin/remove/reorder update the single settings owner via
  // callbacks; window focus + the window list go straight through the bridge.
  const {
    path,
    label,
    isTemp,
    recentProjects,
    pinnedProjects,
    labels,
    onopen,
    onswitch,
    ontogglepin,
    onclearrecent,
    onremoverecent,
    ondelete,
    onreorderpins
  }: {
    path: string;
    label: string;
    isTemp: boolean;
    recentProjects: string[];
    pinnedProjects: string[];
    labels: Record<string, string>;
    /** Switch this window to `path` (in place). */
    onopen: (path: string) => void;
    /** Open the full picker (browse every root, clone, open a folder). */
    onswitch: () => void;
    /** Pin or unpin a project — persisted by the parent (single settings owner).
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
  } = $props();

  let filter = $state("");
  // Per-project language kind + branch, and the open-window list, all fetched when
  // the menu opens. Missing entries fall back to a folder glyph / no branch.
  let kinds = $state<Record<string, string>>({});
  let branches = $state<Record<string, string>>({});
  let windowRows = $state<WindowInfo[]>([]);
  // Whether the popover is open — the moment a row first has a real width, so the
  // per-row truncation tooltips (re)measure then rather than while hidden.
  let menuOpen = $state(false);
  // The rows container, so list mutations animate through a view transition
  // scoped to *just* this element (never the document — that would snapshot the
  // live-repainting terminal and ghost it).
  let switchListElement = $state<HTMLElement | null>(null);

  // The "Delete directory" confirmation lives inside this menu (a nested popover)
  // so the switcher stays visible behind it; the parent only performs the removal.
  let deletePending = $state<string | null>(null);
  let deleteBusy = $state(false);
  let deleteError = $state("");

  const pinnedSet = $derived(new Set(pinnedProjects.map(normalizePath)));
  // Recents minus anything already pinned, so a project shows in one section only.
  const recentsOnly = $derived(recentProjects.filter(project => !pinnedSet.has(normalizePath(project))));

  function matchesFilter(project: string): boolean {
    const query = filter.trim().toLowerCase();
    if (!query) {
      return true;
    }

    return (
      displayName({ path: project, labels }).toLowerCase().includes(query) ||
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
  // A project's language logo, or the neutral folder glyph when no kind is known.
  function kindIcon(project: string): IconName {
    const kind = kinds[project];
    return kind ? languageIcon(kind) : "folder";
  }
  // Stable, valid popover id/anchor per row kebab (sanitised from the path).
  function rowMenuId(project: string): string {
    return `sw-${project.replaceAll(/[^a-zA-Z0-9]/g, "-")}`;
  }
  // Stable, unique view-transition-name per row, so the scoped view transition
  // tracks each row across a list change and morphs it (a moved row glides to its
  // new slot) instead of cross-fading the whole list. The same project keeps the
  // same name across sections, so pinning it glides from Recent to Pinned.
  function rowTransitionName(project: string): string {
    return `row-${project.replaceAll(/[^a-zA-Z0-9]/g, "-")}`;
  }

  // Fetch the open windows, then kinds + branches for everything the menu shows,
  // in one pass per open. A hiccup (backend restarting mid-dev, a path since
  // removed) just leaves the rows on their folder-glyph fallback rather than
  // throwing.
  async function loadMeta() {
    try {
      const openWindows = await windows.list();
      windowRows = openWindows;
      const paths = [
        ...new Set([path, ...pinnedProjects, ...recentProjects, ...openWindows.map(w => w.path)])
      ].filter(Boolean);
      if (paths.length === 0) {
        return;
      }

      const [detectedKinds, detectedBranches] = await Promise.all([
        ide.projectKinds(paths),
        vcs.branchOf(paths)
      ]);
      kinds = detectedKinds;
      branches = detectedBranches;
    } catch {
    // Leave the last-known metadata in place; rows fall back to folders.
    }
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
    const menu = document.getElementById("m-app");
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
        mode: "open",
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
    requestAnimationFrame(() => document.getElementById("m-app-q")?.focus());
  }

  // Spawn a fresh window and dismiss the menu so it doesn't linger over the new
  // one. `path` is optional — omitted for empty/temp modes.
  async function spawn(args: {
    mode: "empty" | "temp" | "open";
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
  async function animateListChange(mutate: () => Promise<void>) {
    const list:
      | (HTMLElement & {
        startViewTransition?: (callback: () => Promise<void>) => { updateCallbackDone: Promise<void> };
      })
      | null = switchListElement;
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
    const menu = document.getElementById("m-app");
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
</script>

<span class="menu-host">
  <button
    class="trigger menu-trigger"
    aria-haspopup="menu"
    aria-label={`Switch project · Ctrl P — ${label}`}
    popovertarget="m-app"
  >
    <Logo size={18} />
    <span class="stack">
      <span class="eyebrow">Project</span>
      <span class="name">
        {label}
        {#if isTemp}
          <span class="temp">temp</span>
        {/if}
      </span>
    </span>
    <span class="caret" aria-hidden="true">▾</span>
  </button>

  <div
    id="m-app"
    style:position-anchor="--m-app"
    class="menu popover-menu"
    ontoggle={async e => {
      menuOpen = (e as ToggleEvent).newState === "open";

      if (menuOpen) {
        focusFilter();
        await loadMeta();
      }
    }}
    popover
    role="menu"
  >
    <!-- Open PADE windows — in creation order, which is also the cycle order for
       Ctrl+Alt+[ / ]. Click a non-current one to focus its window. -->
    {#if windowRows.length > 0}
      <div class="eyebrow section">Open windows</div>
      {#each windowRows as w (w.label)}
        <!-- The row wraps a separate grip (a span, so grabbing it never triggers the
             button's focus onclick) and the focus button — data-window-id makes the
             wrapper the reorder engine's drag sibling. -->
        <div class="wrow-item" data-window-id={w.label}>
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
            class="wrow"
            class:current={w.isCurrent}
            onclick={() => {
              if (!w.isCurrent) {
                hide();
                windows.focus(w.label);
              }
            }}
            role="menuitem"
            type="button"
          >
            <span class="kind-logo" aria-hidden="true" data-brand={kinds[w.path] ? kindIcon(w.path) : undefined}>
              <Icon name={kindIcon(w.path)} size={16} />
            </span>
            <span class="wrow-name">{shortDisplayName({ path: w.path, labels })}</span>
            {#if isTemporaryWorkspace(w.path)}
              <span class="temp">temp</span>
            {/if}
            <span class="wrow-spacer"></span>
            {#if w.isCurrent}
              <span class="this-window">this window</span>
            {:else}
              <span class="wrow-focus" aria-hidden="true"><Icon name="external" size={14} /></span>
            {/if}
          </button>
        </div>
      {/each}
      <div class="sep"></div>
    {/if}

    <!-- Filter / quick-switch -->
    <label class="search">
      <span class="lead" aria-hidden="true"><Icon name="search" size={15} /></span>
      <input
        id="m-app-q"
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
        class="prow-kebab"
        aria-haspopup="menu"
        aria-label={`Options for ${displayName({ path: project, labels })}`}
        popovertarget={menuId}
        type="button"
      ><Icon name="more" size={16} /></button>
      <div id={menuId} class="row-menu popover-menu" popover role="menu">
        <button
          class="mi" onclick={() => animateListChange(() => ontogglepin({
            path: project,
            pinned: !pinned
          }))}
          popovertarget={menuId}
          popovertargetaction="hide"
          role="menuitem"
          type="button">
          <span class="mi-ico"><Icon name="star" size={15} /></span>
          <span>{#if pinned}
            Unpin from top{:else}Pin to top{/if}</span>
        </button>
        <button
          class="mi" onclick={() => animateListChange(() => onremoverecent(project))}
          popovertarget={menuId}
          popovertargetaction="hide"
          role="menuitem"
          type="button">
          <span class="mi-ico"><Icon name="close" size={15} /></span>
          <span>Remove from list</span>
        </button>
        <div class="sep"></div>
        <button
          class="mi critical" onclick={() => {
            deleteError = "";
            deletePending = project;
          }}
          popovertarget={menuId}
          popovertargetaction="hide"
          role="menuitem"
          type="button">
          <span class="mi-ico"><Icon name="trash" size={15} /></span>
          <span class="mi-body">
            <span>Delete directory</span>
            <span class="mi-sub">{project}</span>
          </span>
        </button>
      </div>
    {/snippet}

    <!-- The row's main button (logo, name, branch, path), shared by both sections. -->
    {#snippet rowMain(project: string)}
      {@const current = isCurrent(project)}
      <button
        class="prow-main"
        class:current
        aria-checked={current}
        onclick={e => pick(project, e)}
        role="menuitemradio"
        type="button"
      >
        <span class="kind-logo" aria-hidden="true" data-brand={kinds[project] ? kindIcon(project) : undefined}>
          <Icon name={kindIcon(project)} size={16} />
        </span>
        <span class="prow-body">
          <span class="prow-name-row">
            <span class="prow-name">{displayName({ path: project, labels })}</span>
            {#if isTemporaryWorkspace(project)}
              <span class="temp">temp</span>
            {/if}
          </span>
          <span class="prow-meta">
            {#if branches[project]}
              <span class="branch">
                <span class="branch-icon" aria-hidden="true"><Icon name="branch" size={11} /></span>
                {branches[project]}
              </span>
            {/if}
            <span
              class="prow-path" {@attach truncationTooltip({
                text: project,
                visible: menuOpen
              })}>{project}</span>
          </span>
        </span>
        {#if current}
          <span class="prow-check" aria-hidden="true"><Icon name="check" size={15} /></span>
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
            class="prow"
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
          <div style:view-transition-name={rowTransitionName(project)} class="prow">
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

    <div class="sep"></div>

    <button
      class="action" onclick={() => {
        hide();
        onswitch();
      }} role="menuitem" type="button">
      <span class="lead"><Icon name="swap" /></span>
      <span class="grow">Open a project…</span>
      <span class="sub">All projects &amp; clone</span>
    </button>

    <div class="sep"></div>

    <div class="eyebrow section">New window</div>
    <button class="action" onclick={() => spawn({ mode: "empty" })} role="menuitem" type="button">
      <span class="lead accent"><Icon name="windowPlus" /></span>
      <span class="grow">Empty window</span>
      <span class="kbd">Ctrl ⇧ N</span>
    </button>
    <button class="action" onclick={() => spawn({ mode: "temp" })} role="menuitem" type="button">
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
            <span class="target-name">{displayName({ path: target, labels })}</span>
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
    anchor-name: --m-app;

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
  .temp {
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

  /* A language logo carries its brand colour (theme.css [data-brand]); a folder
     fallback (no data-brand) reads muted. It keeps its colour on hover — a
     baked-colour brand SVG (JS) can't recolour, so none of them do. */
  .kind-logo {
    display: inline-flex;
    flex: none;
    color: var(--brand-color, var(--on-surface-variant));
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
     like a pinned .prow so the shared .grip sits flush to the row's left edge. */
  .wrow-item {
    position: relative;
    display: flex;
    gap: 2px;
    align-items: center;
  }

  /* An open-window row: focus another window, or "this window" for the current one. */
  .wrow {
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

    .wrow-name {
      overflow: hidden;
      min-inline-size: 0;
      font-family: var(--font-monospace);
      font-weight: 600;
      font-size: 13px;
      text-overflow: ellipsis;
      white-space: nowrap;
    }

    .wrow-spacer {
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

    .wrow-focus {
      display: inline-flex;
      flex: none;
      color: var(--on-surface-variant);
    }

    &:hover .wrow-focus,
    &:focus-visible .wrow-focus {
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

  .prow {
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

  .prow-main {
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

  .prow-body {
    display: flex;
    flex: 1;
    flex-direction: column;
    min-inline-size: 0;
    line-height: 1.25;
  }

  .prow-name-row {
    display: flex;
    gap: 6px;
    align-items: center;
    min-inline-size: 0;

    .prow-name {
      overflow: hidden;
      font-family: var(--font-monospace);
      font-weight: 600;
      font-size: 13px;
      text-overflow: ellipsis;
      white-space: nowrap;
    }
  }

  .prow-meta {
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

    .prow-path {
      overflow: hidden;
      min-inline-size: 0;
      color: var(--on-surface-variant);
      font-family: var(--font-monospace);
      font-size: 9px;
      text-overflow: ellipsis;
      white-space: nowrap;
    }
  }

  .prow-main:hover .prow-meta,
  .prow-main:focus-visible .prow-meta,
  .prow-main:hover .prow-meta .branch,
  .prow-main:focus-visible .prow-meta .branch {
    color: inherit;
  }

  .prow-check {
    display: inline-flex;
    flex: none;
    color: var(--primary);
  }

  .prow-main:hover .prow-check,
  .prow-main:focus-visible .prow-check {
    color: inherit;
  }

  /* Row kebab — the ⋮ button opening the per-row options popover. */
  .prow-kebab {
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
  .prow:hover .prow-kebab,
  .prow:focus-within .prow-kebab {
    opacity: 100%;
  }

  /* Per-row options popover — Pin/Unpin, Remove, Delete. Width is capped so the
     Delete item's path ellipsizes (.mi-sub) instead of ballooning the menu wide
     enough to spill past the panel over the terminal. */
  .row-menu {
    min-inline-size: 210px;
    max-inline-size: 260px;
    position-area: bottom span-left;

    .mi {
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

      .mi-ico {
        display: inline-flex;
        flex: none;
        color: var(--on-surface-variant);
      }

      &:hover .mi-ico,
      &:focus-visible .mi-ico {
        color: inherit;
      }

      .mi-body {
        display: flex;
        flex-direction: column;
        gap: 1px;
        min-inline-size: 0;
      }

      .mi-sub {
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
    .mi.critical {
      color: var(--critical);

      .mi-ico {
        color: var(--critical);
      }

      &:hover,
      &:focus-visible {
        background: var(--critical-wash);
        color: var(--critical);
      }

      &:hover .mi-ico,
      &:focus-visible .mi-ico,
      &:hover .mi-sub,
      &:focus-visible .mi-sub {
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

  .sep {
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

    .sub {
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
    &:hover .sub,
    &:focus-visible .sub,
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
