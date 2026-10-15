<script lang="ts">
  import type { WindowInfo } from "@/lib/types";
  import { isOpenInAnotherWindow } from "@/lib/workspace/open-windows";
  import ProjectRowMain from "@/lib/workspace/switcher/ProjectRowMain.svelte";
  import ProjectRowMenu from "@/lib/workspace/switcher/ProjectRowMenu.svelte";
  import ProjectSection from "@/lib/workspace/switcher/ProjectSection.svelte";
  import ReorderGrip from "@/lib/workspace/switcher/ReorderGrip.svelte";
  import { isCurrentProject, rowTransitionName } from "@/lib/workspace/switcher/switcher-rows";

  // The switcher's scrolling Pinned and Recent sections (already narrowed by the
  // filter). Every row action only reports back: the switcher runs the list
  // mutations through its scoped view transition and owns the nested dialogs.
  let {
    pinnedShown,
    recentShown,
    isFiltered,
    path,
    labels,
    openedAt,
    windowRows,
    elList = $bindable(null),
    onpick,
    ontogglepin,
    onrelabel,
    onrenamefolder,
    onremove,
    ondelete,
    onclearrecent,
    onreorderpins
  }: {
    pinnedShown: string[];
    /** Recents minus anything pinned, so a project shows in one section only. */
    recentShown: string[];
    /** A filter narrows the rows. */
    isFiltered: boolean;
    /** This window's project. */
    path: string;
    labels: Record<string, string>;
    openedAt: number;
    /** The open windows - a project open in another one can't be renamed. */
    windowRows: WindowInfo[];
    /** The rows container, the scope of the switcher's list view transition. */
    elList?: HTMLElement | null;
    onpick: (target: {
      project: string;
      event: MouseEvent;
    }) => void;
    ontogglepin: (target: {
      path: string;
      pinned: boolean;
    }) => void;
    onrelabel: (project: string) => void;
    onrenamefolder: (project: string) => void;
    onremove: (project: string) => void;
    ondelete: (project: string) => void;
    onclearrecent: () => void;
    onreorderpins: (paths: string[]) => void;
  } = $props();

  // Pins reorder only when there's more than one and no filter narrows the set -
  // the drag engine commits the visible rows, so a filtered subset would drop the
  // hidden pins from the saved order.
  const isPinListReorderable = $derived(!isFiltered && pinnedShown.length > 1);
  const isFilterUnmatched = $derived(isFiltered && pinnedShown.length === 0 && recentShown.length === 0);
</script>

{#snippet row(project: string, isPinned: boolean)}
  <ProjectRowMain
    isCurrent={isCurrentProject({
      project,
      current: path
    })}
    {labels}
    onpick={e => onpick({
      project,
      event: e
    })}
    {openedAt}
    {project}
  />
  <ProjectRowMenu
    {isPinned}
    isRenamable={!isOpenInAnotherWindow({
      windowRows,
      path: project
    })}
    {labels}
    ondelete={() => ondelete(project)}
    onrelabel={() => onrelabel(project)}
    onremove={() => onremove(project)}
    onrenamefolder={() => onrenamefolder(project)}
    ontogglepin={() => ontogglepin({
      path: project,
      pinned: !isPinned
    })}
    {project}
  />
{/snippet}

<!-- view-transition-name lets the scoped view transition morph THIS row (a
     moved/removed row glides) instead of cross-fading the whole list;
     data-pin-id keeps a flat drag-sibling set for the reorder engine. -->
{#snippet pinnedRow(project: string)}
  <div
    style:view-transition-name={rowTransitionName(project)}
    class="project-row"
    data-pin-id={isFiltered ? undefined : project}
  >
    <ReorderGrip
      idAttribute="data-pin-id"
      isEnabled={isPinListReorderable}
      itemSelector="[data-pin-id]"
      oncommit={onreorderpins}
    />
    {@render row(project, true)}
  </div>
{/snippet}

{#snippet recentRow(project: string)}
  <div style:view-transition-name={rowTransitionName(project)} class="project-row">
    {@render row(project, false)}
  </div>
{/snippet}

<div bind:this={elList} class="switch-list">
  <ProjectSection projects={pinnedShown} row={pinnedRow} title="Pinned" />
  <ProjectSection onclear={onclearrecent} projects={recentShown} row={recentRow} title="Recent" />

  {#if isFilterUnmatched}
    <div class="no-results">
      No open projects match. Try <strong>Open a project…</strong> below.
    </div>
  {/if}
</div>

<style>
  .switch-list {
    display: flex;
    flex-direction: column;
    gap: 1px;
    overflow-y: auto;
    max-block-size: min(46vh, 320px);
    margin-inline: -1px;
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

    /* Reveal the row's kebab on hover/focus (or while its own menu is open). */
    &:hover,
    &:focus-within {
      --row-kebab-opacity: 100%;
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
</style>
