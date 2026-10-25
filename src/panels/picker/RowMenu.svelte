<script lang="ts">
  import Icon from "@/lib/Icon.svelte";
  import { Command, sendMessage } from "@/lib/ipc/messaging";
  import type { WorkspaceLifecycle } from "@/panels/picker/lifecycle.svelte";
  import OwnedWorkspaceItems from "@/panels/picker/OwnedWorkspaceItems.svelte";

  // Trailing kebab + actions popover for one project row: reveal in Files /
  // Terminal, renaming the folder (any project no other window holds), deleting
  // the folder (any project on the page), and - for PADE-owned workspaces - the
  // relabel (temp only) / rename / move lifecycle. Opening in an editor is its
  // own visible row button (OpenInEditorButton), so it isn't repeated here.
  // Chrome comes from picker/chrome.css.
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
  const deleteLabel = $derived.by(() => {
    if (isOwned) {
      return "Delete workspace";
    }

    return "Delete directory";
  });
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
    <li class="menu-divider" role="separator"></li>
    <li class="menu-separator">Workspace</li>
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
    <li>
      <!-- Shift-click is the power-user path: it skips the confirmation and
           deletes straight away (a failure still surfaces in the dialog). -->
      <button
        class="menu-item danger"
        onclick={async e => {
          if (e.shiftKey) {
            await lifecycle.deleteNow(path);
            return;
          }

          lifecycle.requestDelete(path);
        }}
        popovertarget={identifier}
        popovertargetaction="hide"
      >
        <Icon name="trash" /><span class="menu-item-text">{deleteLabel}</span><kbd>⇧</kbd>
      </button>
    </li>
  </ul>
</span>

<style>
  /* Layout-neutral wrapper so the shared .menu-host rule can hold the kebab
     active while this row's menu is open. */
  .menu-host {
    display: contents;
  }

  /* Trailing kebab - a subtle pill circle that fills on hover. */
  .kebab {
    display: inline-flex;
    flex: none;
    justify-content: center;
    align-items: center;
    block-size: 30px;
    inline-size: 30px;
    margin-inline-start: auto;
    padding: 0;
    border: none;
    border-radius: var(--radius-full);
    background: transparent;
    color: var(--on-surface-variant);
    cursor: pointer;
    transition: background 150ms var(--ease);

    &:hover {
      background: var(--surface-2);
      color: var(--on-surface);
    }
  }

  .menu {
    /* Modifier hint on an item that takes a shortcut (Shift = skip the
       confirmation). A tooltip would be clipped by the menu's own overflow. */
    .menu-item kbd {
      flex: none;
      padding-block: 1px;
      padding-inline: 5px;
      border-radius: var(--radius-small);
      background: var(--surface-3);
      color: var(--on-surface-variant);
      font-family: var(--font-ui);
      font-size: 0.625rem;
    }

    /* Destructive item reads critical-red at rest (not only on hover), so a
       delete action is legible as dangerous before the pointer lands. */
    .danger {
      color: var(--critical);

      &:hover {
        background: var(--critical-wash);
        color: var(--critical);
      }
    }
  }
</style>
