<script lang="ts">
  import Icon from "@/lib/Icon.svelte";
  import { baseName } from "@/lib/util/paths";
  import RootedName from "@/lib/workspace/RootedName.svelte";
  import { THROWAWAY_LABEL } from "@/panels/picker/home-composer";
  import { tick } from "svelte";

  // The composer menu's New section, the foot under the scrolling list so it
  // is one glance away however long the list runs: a new project under each
  // root, or a throwaway workspace.
  const { menuId, roots, isSeparated, isTempPicked, nameFieldId, onnew, ontemp }: {
    menuId: string;
    roots: readonly string[];
    isSeparated: boolean;
    isTempPicked: boolean;
    /** The name field a new project focuses once the menu closes. */
    nameFieldId: string;
    /** Name a new project under `root` (empty when no root is set up yet). */
    onnew: (root: string) => void;
    ontemp: () => void;
  } = $props();

  // With no root set up, one "New project…" leads to the form's own root picker.
  const newTargets = $derived(roots.length > 0 ? roots : [""]);

  // Hide the menu imperatively (like RootSelect's jump-to-add): the
  // declarative hide would run after this handler and cancel the focus into
  // the name field.
  async function startNew(e: MouseEvent, root: string) {
    if (e.currentTarget instanceof HTMLElement) {
      e.currentTarget.closest<HTMLElement>("[popover]")?.hidePopover();
    }

    onnew(root);
    await tick();
    document.getElementById(nameFieldId)?.focus();
  }
</script>

<div class="new-section" class:separator={isSeparated}>
  <span class="head" aria-hidden="true">New</span>
  <div class="roots">
    {#each newTargets as root (root)}
      <button class="menu-item create-option" onclick={async e => await startNew(e, root)} type="button">
        <Icon name="folderPlus" size={15} />
        {#if root}
          <span class="menu-item-text">New project in <RootedName name="" root={baseName(root)} /></span>
        {:else}
          <span class="menu-item-text">New project…</span>
        {/if}
      </button>
    {/each}
  </div>
  <button
    class="menu-item create-option"
    class:picked={isTempPicked}
    onclick={ontemp}
    popovertarget={menuId}
    popovertargetaction="hide"
    type="button"
  >
    <Icon name="plus" size={15} />
    <span class="menu-item-text">{THROWAWAY_LABEL}</span>
  </button>
</div>

<style>
  /* The menu's non-scrolling foot (HomeProjectMenu lays the menu out as a
     column): it keeps its full size while the list above it scrolls. */
  .new-section {
    display: flex;
    flex: none;
    flex-direction: column;
  }

  /* With four roots or more, three and a half rows show and the rest scroll,
     so the section never crowds the project list out - the half row says
     there is more. A row is one 13px line plus the menu item's 8px top and
     bottom padding (theme.css `.menu-item`). */
  .roots {
    display: flex;
    flex-direction: column;
    font-size: 0.8125rem;

    &:has(> :nth-child(4)) {
      overflow-y: auto;
      max-block-size: calc(3.5 * (1lh + 2 * 8px));
      scrollbar-width: thin;
    }
  }

  /* The create actions read primary among the plain project rows. */
  .create-option {
    color: var(--primary);
    font-weight: 600;
  }
</style>
