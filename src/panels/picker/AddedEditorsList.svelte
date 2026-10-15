<script lang="ts">
  import Icon from "@/lib/Icon.svelte";
  import { showToast } from "@/lib/stores/ui/toast.svelte";
  import type { Prefs } from "@/lib/types";

  // Editors located by executable path - listed so a stale one can be dropped
  // from every menu.
  const { editors, onremoveeditor }: {
    editors: NonNullable<Prefs["addedEditors"]>;
    onremoveeditor: (id: string) => Promise<void>;
  } = $props();
</script>

{#if editors.length > 0}
  <div class="editor-added">
    <span class="editor-added-eyebrow">Added manually</span>
    {#each editors as editor (editor.id)}
      <div class="editor-added-row">
        <span class="editor-added-information">
          <span class="editor-added-name-row">
            <span class="editor-added-name">{editor.label}</span>
            <span class="editor-added-tag">added</span>
          </span>
          <span class="editor-added-path" data-tooltip={editor.path}>{editor.path}</span>
        </span>
        <button
          class="editor-remove"
          aria-label={`Remove ${editor.label}`}
          data-tooltip="Remove"
          onclick={async () => {
            await onremoveeditor(editor.id);
            showToast(`Removed ${editor.label}`);
          }}
          type="button"
        ><Icon name="trash" size={15} /></button>
      </div>
    {/each}
  </div>
{/if}

<style>
  /* ── "Added manually" - editors located by path, listed for removal. ── */
  .editor-added {
    display: flex;
    flex-direction: column;
    gap: 6px;
  }

  .editor-added-eyebrow {
    color: var(--on-surface-variant);
    font-weight: 700;
    font-size: 10px;
    letter-spacing: 0.06em;
    text-transform: uppercase;
  }

  .editor-added-row {
    display: flex;
    gap: 10px;
    align-items: center;
    padding-block: 8px;
    padding-inline: 12px 8px;
    border: 1px solid var(--outline);
    border-radius: var(--radius-medium);
    background: var(--surface-2);
  }

  .editor-added-information {
    display: flex;
    flex: 1;
    flex-direction: column;
    gap: 2px;
    min-inline-size: 0;
  }

  .editor-added-name-row {
    display: flex;
    gap: 8px;
    align-items: center;
  }

  .editor-added-name {
    font-weight: 600;
    font-size: 13px;
  }

  /* "added" provenance tag - a quiet tertiary micro-label, no fill. */
  .editor-added-tag {
    flex: none;
    color: var(--tertiary);
    font-weight: 700;
    font-size: 9px;
    letter-spacing: 0.05em;
    text-transform: uppercase;
  }

  .editor-added-path {
    overflow: hidden;
    max-inline-size: 100%;
    color: var(--on-surface-variant);
    font-family: var(--font-monospace);
    font-size: 10px;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  /* Remove - a circle that reddens on hover, like the roots-row close button. */
  .editor-remove {
    display: inline-flex;
    flex: none;
    justify-content: center;
    align-items: center;
    block-size: 28px;
    inline-size: 28px;
    padding: 0;
    border: none;
    border-radius: var(--radius-full);
    background: transparent;
    color: var(--on-surface-variant);
    cursor: pointer;
    transition:
      background 150ms var(--ease),
      color 150ms var(--ease);

    &:hover {
      background: var(--critical-wash);
      color: var(--critical);
      filter: none;
    }
  }
</style>
