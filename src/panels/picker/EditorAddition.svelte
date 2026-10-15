<script lang="ts">
  import Icon from "@/lib/Icon.svelte";
  import { showToast } from "@/lib/stores/ui/toast.svelte";
  import type { Prefs } from "@/lib/types";
  import AddedEditorsList from "@/panels/picker/AddedEditorsList.svelte";
  import AddEditorForm from "@/panels/picker/AddEditorForm.svelte";

  // "Add editor…" - locate an editor PADE didn't auto-detect on PATH: reveal an
  // inline path field, validate & persist the executable through the backend,
  // and surface an ok/error status line. The manually added editors are listed
  // below for removal.
  const { addedEditors, onaddeditor, onremoveeditor }: {
    addedEditors: NonNullable<Prefs["addedEditors"]>;
    onaddeditor: (path: string) => Promise<{
      label: string;
    } | {
      error: string;
    }>;
    onremoveeditor: (id: string) => Promise<void>;
  } = $props();

  const StatusKind = {
    Ok: "ok",
    Error: "err"
  } as const;
  type StatusKind = (typeof StatusKind)[keyof typeof StatusKind];

  let isAdding = $state(false);
  let status = $state<{
    kind: StatusKind;
    text: string;
  } | null>(null);
  const isStatusError = $derived(status?.kind === StatusKind.Error);
  const statusIcon = $derived.by(() => {
    if (isStatusError) {
      return "alert";
    }

    return "check";
  });
</script>

<div class="editor-addition">
  <div class="editor-addition-header">
    <span class="editor-addition-icon" aria-hidden="true"><Icon name="monitor" /></span>
    <span class="editor-addition-copy">
      <strong>Using an editor that isn’t listed?</strong>
      <small>
        PADE lists the editors it found automatically. Point it at any other
        editor’s executable and it’ll appear in the menus above.
      </small>
    </span>
    {#if !isAdding}
      <button
        class="editor-addition-button"
        onclick={() => {
          isAdding = true;
          status = null;
        }}
        type="button"
      >
        <Icon name="plus" /> <span>Add editor…</span>
      </button>
    {/if}
  </div>

  {#if isAdding}
    <AddEditorForm
      onadded={label => {
        status = {
          kind: StatusKind.Ok,
          text: `${label} added.`
        };
        showToast(`${label} added`);
        isAdding = false;
      }}
      {onaddeditor}
      oncancel={() => {
        isAdding = false;
        status = null;
      }}
      onedit={() => (status = null)}
      onrejected={reason => {
        status = {
          kind: StatusKind.Error,
          text: reason
        };
      }}
    />
  {/if}

  {#if status}
    <output class="editor-status" class:error={isStatusError}>
      <span class="editor-status-icon" aria-hidden="true">
        <Icon name={statusIcon} size={14} />
      </span>
      <span>{status.text}</span>
    </output>
  {/if}

  <AddedEditorsList editors={addedEditors} {onremoveeditor} />
</div>

<style>
  .editor-addition {
    display: flex;
    flex-direction: column;
    gap: 8px;
    padding: 12px 14px;
    border: 1px solid var(--outline);
    border-radius: var(--radius-medium);
    background: var(--surface-1);
  }

  .editor-addition-header {
    display: flex;
    flex-wrap: wrap;
    gap: 12px;
    align-items: center;
  }

  .editor-addition-icon {
    display: inline-flex;
    flex: none;
    color: var(--primary);
  }

  .editor-addition-copy {
    display: flex;
    flex: 1;
    flex-direction: column;
    gap: 2px;
    min-inline-size: 0;

    strong {
      font-weight: 700;
      font-size: 13px;
    }

    small {
      color: var(--on-surface-variant);
      font-size: 12px;
      line-height: 1.45;
    }
  }

  /* Reveal button - tonal surface-3 pill that fills primary-container on hover. */
  .editor-addition-button {
    display: inline-flex;
    flex: none;
    gap: 6px;
    align-items: center;
    padding: 7px 12px;
    border: none;
    border-radius: var(--radius-small);
    background: var(--surface-3);
    color: var(--on-surface);
    font: inherit;
    font-weight: 700;
    font-size: 12px;
    cursor: pointer;
    transition:
      background 150ms var(--ease),
      color 150ms var(--ease);

    &:hover {
      background: var(--primary-container);
      color: var(--on-primary-container);
    }
  }

  /* Inline result - tertiary wash on success, crit wash on rejection. */
  .editor-status {
    display: flex;
    gap: 7px;
    align-items: flex-start;
    padding: 7px 9px;
    border-radius: var(--radius-small);
    background: var(--tertiary-wash);
    color: var(--tertiary);
    font-size: 11px;
    line-height: 1.45;
    animation: line-in 180ms var(--ease);

    &.error {
      background: var(--critical-wash);
      color: var(--critical);
    }

    .editor-status-icon {
      display: inline-flex;
      flex: none;
      margin-block-start: 1px;
    }
  }
</style>
