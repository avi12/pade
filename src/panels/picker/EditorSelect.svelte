<script lang="ts">
  import Icon from "@/lib/Icon.svelte";
  import { ideBrand, ideIcon } from "@/lib/ide/ide-icon";
  import type { Ide } from "@/lib/types";
  import { editorSelectId } from "@/panels/picker/editor-options";

  // One editor-rule select - a native-popover menu of the editors that suit a
  // project kind (or "fallback"), anchored to its trigger. `ides` is every
  // detected editor: the picked one is looked up there, so a rule pointing at
  // an editor that's no longer installed reads "Choose…" with no icon.
  const { key, value, options, ides, onpick, ariaLabel }: {
    key: string;
    value: string;
    options: Ide[];
    ides: Ide[];
    onpick: (editorId: string) => void;
    ariaLabel: string;
  } = $props();

  const selectId = $derived(editorSelectId(key));
  const pickedEditor = $derived(ides.find(editor => editor.id === value));
  const triggerLabel = $derived(pickedEditor?.label ?? "Choose…");
  const isEmpty = $derived(ides.length === 0);
</script>

<span class="editor-selector menu-host">
  <button
    style:anchor-name="--{selectId}"
    class="editor-trigger field-trigger menu-trigger"
    aria-label={ariaLabel}
    disabled={isEmpty}
    popovertarget={selectId}
    type="button"
  >
    {#if pickedEditor}
      <span class="editor-icon" aria-hidden="true" data-brand={ideBrand(pickedEditor.id)}>
        <Icon name={ideIcon(pickedEditor.id)} size={15} />
      </span>
    {/if}
    <span>{triggerLabel}</span>
    <span class="caret" aria-hidden="true">▾</span>
  </button>
  <ul id={selectId} style:position-anchor="--{selectId}" class="menu editor-menu popover-menu" popover>
    {#each options as editor (editor.id)}
      {@const isPicked = editor.id === value}
      <li>
        <button
          class="menu-item editor-option"
          class:picked={isPicked}
          aria-current={isPicked}
          onclick={() => onpick(editor.id)}
          popovertarget={selectId}
          popovertargetaction="hide"
          type="button"
        >
          <span class="option-label">
            <span class="editor-icon" aria-hidden="true" data-brand={ideBrand(editor.id)}>
              <Icon name={ideIcon(editor.id)} size={15} />
            </span>
            <span>{editor.label}</span>
          </span>
          {#if isPicked}
            <span class="tick" aria-hidden="true">✓</span>
          {/if}
        </button>
      </li>
    {:else}
      <li class="none editor-empty">No editors detected</li>
    {/each}
  </ul>
</span>

<style>
  .editor-selector {
    position: relative;
    flex: none;
  }

  /* Popover select trigger - pill that brightens its edge on hover. */
  .editor-trigger {
    display: inline-flex;
    gap: 8px;
    align-items: center;
    padding: 8px 12px;
    transition:
      border-color 150ms var(--ease),
      background 150ms var(--ease);

    &:hover:not(:disabled) {
      background: var(--surface-3);
      filter: none;
    }

    .caret {
      display: inline-block;
      font-size: 9px;
      opacity: 70%;
      transition: rotate 150ms var(--ease);
    }
  }

  /* Editor brand mark in the trigger and each option. */

  /* The editor's brand colour; a black brand (JetBrains) follows the text. */
  .editor-icon {
    display: inline-flex;
    flex: none;
    color: var(--brand-color, currentColor);
  }

  /* Reuse the row-menu popover chrome; align + size for a select. */
  .editor-menu {
    min-inline-size: 180px;

    .editor-option {
      justify-content: space-between;
      font-weight: 600;
    }

    .option-label {
      display: inline-flex;
      gap: 8px;
      align-items: center;
    }
  }

  .editor-empty {
    padding: 8px 10px;
  }
</style>
