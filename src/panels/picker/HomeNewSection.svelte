<script lang="ts">
  import Icon from "@/lib/Icon.svelte";
  import { baseName } from "@/lib/util/paths";
  import RootedName from "@/lib/workspace/RootedName.svelte";
  import { THROWAWAY_LABEL } from "@/panels/picker/home-composer";
  import { tick } from "svelte";

  // The composer menu's New section, pinned to the foot of the scrolling list
  // so it is one glance away however long the list runs: a new project under
  // each root, or a throwaway workspace.
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
      e.currentTarget.closest("ul")?.hidePopover();
    }

    onnew(root);
    await tick();
    document.getElementById(nameFieldId)?.focus();
  }
</script>

<li class="new-section" class:separator={isSeparated}>
  <span class="head" aria-hidden="true">New</span>
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
</li>

<style>
  /* Stays at the foot of the scrolling list, over the rows that scroll
     beneath it, in the menu's own surface. */
  .new-section {
    position: sticky;
    inset-block-end: 0;
    display: flex;
    flex-direction: column;
    background: var(--surface-2);
  }

  /* The create actions read primary among the plain project rows. */
  .create-option {
    color: var(--primary);
    font-weight: 600;
  }
</style>
