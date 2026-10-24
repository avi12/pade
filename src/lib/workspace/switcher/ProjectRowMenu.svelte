<script lang="ts">
  import Icon from "@/lib/Icon.svelte";
  import { Command, sendMessage } from "@/lib/ipc/messaging";
  import { showToast } from "@/lib/stores/ui/toast.svelte";
  import { rovingMenu } from "@/lib/ui/roving-menu";
  import { displayName, isTemporaryWorkspace } from "@/lib/util/paths";
  import { rowMenuId } from "@/lib/workspace/switcher/switcher-rows";

  // A switcher row's kebab and the options popover it opens, shared by the
  // Pinned and Recent sections. Every list action only reports back - the
  // switcher owns the list transition and the nested dialogs. Browsing files
  // touches neither, so it opens the folder itself.
  const {
    project,
    labels,
    isPinned,
    isRenamable,
    ontogglepin,
    onrelabel,
    onrenamefolder,
    onremove,
    ondelete
  }: {
    project: string;
    labels: Record<string, string>;
    isPinned: boolean;
    /** The folder can move - it isn't held open by another window. */
    isRenamable: boolean;
    ontogglepin: () => void;
    onrelabel: () => void;
    onrenamefolder: () => void;
    onremove: () => void;
    ondelete: () => void;
  } = $props();

  const menuId = $derived(rowMenuId(project));
</script>

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
    class="menu-item" onclick={ontogglepin}
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
      class="menu-item" onclick={onrelabel}
      popovertarget={menuId}
      popovertargetaction="hide"
      role="menuitem"
      type="button">
      <span class="menu-item-icon"><Icon name="pencil" size={15} /></span>
      <span>Relabel in PADE</span>
    </button>
  {/if}
  <button
    class="menu-item" onclick={async () => {
      try {
        await sendMessage(Command.openInExplorer, { path: project });
      } catch {
        showToast("Couldn't open that folder");
      }
    }}
    popovertarget={menuId}
    popovertargetaction="hide"
    role="menuitem"
    type="button">
    <span class="menu-item-icon"><Icon name="folderOpen" size={15} /></span>
    <span>Browse files</span>
  </button>
  {#if isRenamable}
    <button
      class="menu-item" onclick={onrenamefolder}
      popovertarget={menuId}
      popovertargetaction="hide"
      role="menuitem"
      type="button">
      <span class="menu-item-icon"><Icon name="folder" size={15} /></span>
      <span>Rename folder…</span>
    </button>
  {/if}
  <button
    class="menu-item" onclick={onremove}
    popovertarget={menuId}
    popovertargetaction="hide"
    role="menuitem"
    type="button">
    <span class="menu-item-icon"><Icon name="close" size={15} /></span>
    <span>Remove from list</span>
  </button>
  <div class="menu-divider"></div>
  <button
    class="menu-item critical" onclick={ondelete}
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

<style>
  /* Row kebab - the ⋮ button opening the per-row options popover. The row it
     sits in reveals it on hover/focus through --row-kebab-opacity. */
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
    opacity: var(--row-kebab-opacity, 55%);
    cursor: pointer;
    transition: color 120ms var(--ease), background 120ms var(--ease), opacity 120ms var(--ease);

    &:hover {
      background: var(--surface-3);
      color: var(--on-surface);
      opacity: 100%;
    }
  }

  /* Per-row options popover - Pin/Unpin, Browse files, Remove, Delete. Width is capped so the
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
</style>
