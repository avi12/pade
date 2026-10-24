<script lang="ts">
  import type { ExtensionCount } from "@/lib/feed/filter/type-filter";
  import { typeFilterLabel } from "@/lib/feed/filter/type-filter";
  import Icon from "@/lib/Icon.svelte";
  import { Command, sendMessage } from "@/lib/ipc/messaging";
  import { showToast } from "@/lib/stores/ui/toast.svelte";
  import { errorHeadline } from "@/lib/util/error-text";
  import TypeFilterMenu from "@/panels/feed/TypeFilterMenu.svelte";
  import type { SvelteSet } from "svelte/reactivity";

  // The feed's toolbar: the opt-in file-type filter (a pill that reads the
  // current selection and drops a native popover of per-extension checkboxes)
  // and "Sync all", which fast-forwards the workspace from its remote.
  const { project, extensionCounts, selectedExtensions, hasRemote }: {
    project: string;
    extensionCounts: ExtensionCount[];
    selectedExtensions: SvelteSet<string>;
    /** There is a remote to pull from - else "Sync all" stays hidden. */
    hasRemote: boolean;
  } = $props();

  // "Sync all" in-flight guard - disables the button and spins its icon so a
  // slow fetch can't be double-fired.
  let isSyncing = $state(false);
</script>

<div class="toolbar">
  <div class="typefilter menu-host">
    <button
      style:anchor-name="--type-anchor"
      class="typechip menu-trigger"
      popovertarget="type-filter"
    >
      <span class="caret">▽</span>
      <span class="label">{typeFilterLabel({
        counts: extensionCounts,
        selected: selectedExtensions
      })}</span>
    </button>
    <TypeFilterMenu id="type-filter" anchor="--type-anchor" {extensionCounts} {selectedExtensions} />
  </div>

  {#if hasRemote}
    <button
      class="sync"
      data-tooltip="Fast-forward this workspace from origin"
      disabled={isSyncing}
      onclick={async () => {
        isSyncing = true;
        try {
          const outcome = await sendMessage(Command.vcsPull, { cwd: project });
          showToast(outcome.message);
        } catch (error) {
          showToast(
            errorHeadline({
              error,
              fallback: "Sync failed"
            })
          );
        } finally {
          isSyncing = false;
        }
      }}
    >
      <span class="icon" class:spin={isSyncing}><Icon name="refresh" size={14} /></span>
      Sync all
    </button>
  {/if}
</div>

<style>
  .toolbar {
    display: flex;
    gap: 8px;
    align-items: center;
    padding-block: 10px 0;
    padding-inline: 10px;
  }

  /* The pressed state + caret flip come from the shared .menu-host/.menu-trigger
     chrome (theme.css). */
  .typefilter {
    display: inline-flex;
  }

  /* Both toolbar actions are outlined pills. */
  .typechip,
  .sync {
    display: inline-flex;
    gap: 6px;
    align-items: center;
    border: 1px solid var(--outline);
    border-radius: var(--radius-full);
    background: var(--surface-2);
    color: var(--on-surface);
    font-family: inherit;
    font-weight: 600;
    font-size: 0.75rem;
    cursor: pointer;
    transition: background 120ms var(--ease);
  }

  .typechip {
    padding-block: 4px;
    padding-inline: 10px 12px;

    &:hover {
      background: var(--surface-3);
    }

    .caret {
      font-size: 0.625rem;
      transition: rotate 200ms var(--ease);
    }
  }

  .sync {
    margin-inline-start: auto;
    padding: 4px 12px;

    &:disabled {
      color: var(--on-surface-variant);
      cursor: default;
    }

    &:hover:not(:disabled) {
      background: var(--surface-3);
    }

    .icon {
      display: inline-flex;
      color: var(--primary);
    }

    /* Reuses the global `spin` keyframe (theme.css); the global
       reduced-motion reset disables it for those who ask. */
    .spin {
      animation: spin 900ms linear infinite;
    }
  }
</style>
