<script lang="ts">
  import ConfirmDialog from "@/lib/dialogs/ConfirmDialog.svelte";
  import { displayName } from "@/lib/util/paths";

  // The confirmation before a project folder is deleted from disk - raised by the
  // picker's row menus and by the app menu's switcher. The caller owns the work:
  // it passes `isBusy` while the removal runs and `error` when it fails.
  const {
    path,
    labels,
    title,
    confirmLabel,
    isBusy,
    error,
    isNested = false,
    isSkipTipShown = false,
    oncancel,
    onconfirm
  }: {
    path: string;
    /** Friendly display labels per path - the one source every row reads. */
    labels: Record<string, string>;
    title: string;
    confirmLabel: string;
    isBusy: boolean;
    error: string | null;
    /** Opened inside an already-open menu popover (see ConfirmDialog). */
    isNested?: boolean;
    /** The caller honours Shift-click on Delete as "skip this next time". */
    isSkipTipShown?: boolean;
    oncancel: () => void;
    onconfirm: () => Promise<void>;
  } = $props();
</script>

<ConfirmDialog
  busy={isBusy}
  busyLabel="Deleting…"
  {confirmLabel}
  danger
  {error}
  icon="trash"
  nested={isNested}
  {oncancel}
  {onconfirm}
  {title}
>
  <div class="delete-body">
    <p>The folder and everything inside it is removed from disk. This can’t be undone</p>
    <p class="target">
      <span class="target-name">{displayName({
        path,
        labels
      })}</span>
      <code>{path}</code>
    </p>
    {#if isSkipTipShown}
      <p class="tip">Hold <kbd>Shift</kbd> when clicking Delete to skip this next time</p>
    {/if}
  </div>
</ConfirmDialog>

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
