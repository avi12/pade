<script lang="ts">
  import Icon from "@/lib/Icon.svelte";
  import { baseName } from "@/lib/util/paths";
  import RootedName from "@/lib/workspace/RootedName.svelte";
  import { THROWAWAY_LABEL } from "@/panels/picker/home-composer";
  import { HomeMenuList } from "@/panels/picker/home-menu-tabs";

  // The composer menu's New tab: a new project under each root, or a
  // throwaway workspace.
  const { menuId, roots, isTempPicked, onnew, ontemp }: {
    menuId: string;
    roots: readonly string[];
    isTempPicked: boolean;
    /** Name a new project under `root` (empty when no root is set up yet);
     *  the name field takes focus once the menu has closed. */
    onnew: (root: string) => void;
    ontemp: () => void;
  } = $props();

  // With no root set up, one "New project…" leads to the form's own root picker.
  const newTargets = $derived(roots.length > 0 ? roots : [""]);
</script>

<div class="new-section">
  <div class="roots">
    {#each newTargets as root (root)}
      <!-- The menu hides imperatively, inside this handler: the declarative
           hide would run after it and hand focus back to the trigger,
           undoing the name field's focus. -->
      <button
        class="menu-item create-option"
        data-arrow-list={HomeMenuList.New}
        onclick={e => {
          e.currentTarget.closest<HTMLElement>("[popover]")?.hidePopover();
          onnew(root);
        }}
        type="button"
      >
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
    data-arrow-list={HomeMenuList.New}
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
  .new-section {
    display: flex;
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
