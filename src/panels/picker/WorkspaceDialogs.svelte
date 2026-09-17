<script lang="ts">
  import ConfirmDialog from "@/lib/ConfirmDialog.svelte";
  import { displayName } from "@/lib/paths";
  import RelabelDialog from "@/lib/RelabelDialog.svelte";
  import type { WorkspaceLifecycle } from "@/panels/picker/lifecycle.svelte";

  // The owned-workspace lifecycle's prompts, raised from either list's row menu:
  // the delete confirmation and the PADE-only relabel. One of each for the whole
  // picker, rendered outside its scrolling page.
  const { lifecycle, labels }: {
    lifecycle: WorkspaceLifecycle;
    /** Friendly display labels per path — the one source every row reads. */
    labels: Record<string, string>;
  } = $props();
</script>

{#if lifecycle.deleteTarget}
  <ConfirmDialog
    busy={lifecycle.deleting}
    busyLabel="Deleting…"
    confirmLabel="Delete workspace"
    danger
    error={lifecycle.deleteError}
    icon="trash"
    oncancel={() => lifecycle.cancelDelete()}
    onconfirm={async () => await lifecycle.confirmDelete()}
    title="Delete this workspace?"
  >
    <div class="delete-body">
      <p>The folder and everything inside it is removed from disk. This can’t be undone.</p>
      <p class="target">
        <span class="target-name">{displayName(lifecycle.deleteTarget, labels)}</span>
        <code>{lifecycle.deleteTarget}</code>
      </p>
      <p class="tip">Hold <kbd>Shift</kbd> when clicking Delete to skip this next time.</p>
    </div>
  </ConfirmDialog>
{/if}

{#if lifecycle.relabelTarget}
  <RelabelDialog
    currentLabel={labels[lifecycle.relabelTarget] ?? ""}
    onclose={() => lifecycle.finishRelabel()}
    path={lifecycle.relabelTarget}
  />
{/if}

<style>
  /* Body of the delete-confirmation dialog (its chrome is ConfirmDialog's). */
  .delete-body {
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

    .tip {
      margin-block-start: 12px;
      font-size: 12px;
    }

    kbd {
      padding-block: 2px;
      padding-inline: 6px;
      border-radius: var(--radius-small);
      background: var(--surface-3);
      color: var(--on-surface);
      font-family: var(--font-ui);
      font-weight: 600;
      font-size: 11px;
    }
  }
</style>
