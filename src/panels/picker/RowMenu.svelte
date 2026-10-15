<script lang="ts">
  import Icon from "@/lib/Icon.svelte";
  import { Command, sendMessage } from "@/lib/ipc/messaging";
  import type { WorkspaceLifecycle } from "@/panels/picker/lifecycle.svelte";
  import OwnedWorkspaceItems from "@/panels/picker/OwnedWorkspaceItems.svelte";

  // Trailing kebab + actions popover for one project row: reveal in Files /
  // Terminal, renaming the folder (any project no other window holds), and - for
  // PADE-owned workspaces - the relabel (temp only) / rename / move / delete
  // lifecycle. Opening in an editor is its own visible row button
  // (OpenInEditorButton), so it isn't repeated here. Chrome comes from
  // picker/chrome.css.
  const { path, scope, lifecycle }: {
    path: string;
    /** Disambiguates a path shown in more than one section (Recent AND under
     *  its root) - the same path would otherwise mint duplicate popover ids
     *  and clicking one kebab would open the wrong menu. */
    scope: string;
    lifecycle: WorkspaceLifecycle;
  } = $props();

  // Stable, valid popover id/anchor per row.
  const identifier = $derived(`menu-${scope}-${path.replaceAll(/[^a-zA-Z0-9]/g, "-")}`);
  const isOwned = $derived(lifecycle.isOwned(path));
  const isFolderRenamable = $derived(lifecycle.canRenameFolder(path));
  const hasWorkspaceActions = $derived(isOwned || isFolderRenamable);
</script>

<span class="menu-host">
  <button
    style:anchor-name="--{identifier}"
    class="kebab menu-trigger"
    aria-label="Row actions"
    popovertarget={identifier}
  ><Icon name="more" /></button>
  <ul id={identifier} style:position-anchor="--{identifier}" class="menu popover-menu" popover>
    <li class="menu-separator">Reveal</li>
    <li>
      <button
        class="menu-item"
        onclick={() => sendMessage(Command.openInExplorer, { path })}
        popovertarget={identifier}
        popovertargetaction="hide"
      >
        <Icon name="folder" /><span class="menu-item-text">Open in Files</span>
      </button>
    </li>
    <li>
      <button
        class="menu-item"
        onclick={() => sendMessage(Command.openInTerminal, { path })}
        popovertarget={identifier}
        popovertargetaction="hide"
      >
        <Icon name="terminal" /><span class="menu-item-text">Open in Terminal</span>
      </button>
    </li>
    {#if hasWorkspaceActions}
      <li class="menu-divider" role="separator"></li>
      <li class="menu-separator">Workspace</li>
    {/if}
    {#if isFolderRenamable}
      <li>
        <button
          class="menu-item"
          onclick={() => lifecycle.startRenameFolder(path)}
          popovertarget={identifier}
          popovertargetaction="hide"
        >
          <Icon name="folder" /><span class="menu-item-text">Rename folder…</span>
        </button>
      </li>
    {/if}
    {#if isOwned}
      <OwnedWorkspaceItems {lifecycle} menuId={identifier} {path} />
    {/if}
  </ul>
</span>

<style>
  /* Layout-neutral wrapper so the shared .menu-host rule can hold the kebab
     active while this row's menu is open. */
  .menu-host {
    display: contents;
  }
</style>
