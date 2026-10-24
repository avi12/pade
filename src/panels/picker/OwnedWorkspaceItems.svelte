<script lang="ts">
  import Icon from "@/lib/Icon.svelte";
  import { isTemporaryWorkspace } from "@/lib/util/paths";
  import type { WorkspaceLifecycle } from "@/panels/picker/lifecycle.svelte";

  // A row menu's actions for a PADE-owned workspace: relabel (temp only),
  // rename into a project and move. The delete item is the row menu's own, so
  // owned and unowned rows share one copy. Each item closes the row's popover.
  const { path, menuId, lifecycle }: {
    path: string;
    /** The row menu's popover id, so each item can hide it. */
    menuId: string;
    lifecycle: WorkspaceLifecycle;
  } = $props();
</script>

{#if isTemporaryWorkspace(path)}
  <li>
    <button
      class="menu-item"
      onclick={() => lifecycle.startRelabel(path)}
      popovertarget={menuId}
      popovertargetaction="hide"
    >
      <Icon name="pencil" /><span class="menu-item-text">Relabel in PADE</span>
    </button>
  </li>
{/if}
<li>
  <button
    class="menu-item"
    onclick={() => lifecycle.startRename(path)}
    popovertarget={menuId}
    popovertargetaction="hide"
  >
    <Icon name="pencil" /><span class="menu-item-text">Rename to a project</span>
  </button>
</li>
<li>
  <button
    class="menu-item"
    onclick={async () => await lifecycle.moveWorkspace(path)}
    popovertarget={menuId}
    popovertargetaction="hide"
  >
    <Icon name="swap" /><span class="menu-item-text">Move…</span>
  </button>
</li>
