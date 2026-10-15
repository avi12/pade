<script lang="ts">
  import type { Snippet } from "svelte";

  // The one-field naming form a workspace prompt shows: what the name changes,
  // the field (focused and selected on open), and the folder it applies to.
  // Shared by the relabel and rename-folder prompts so they read and look alike.
  let { value = $bindable(), label, placeholder, path, onsubmit, children }: {
    value: string;
    /** The field's accessible name. */
    label: string;
    placeholder?: string;
    path: string;
    onsubmit: () => Promise<void>;
    /** What the name changes — shown above the field. */
    children: Snippet;
  } = $props();
</script>

<form
  class="name-field" onsubmit={async e => {
    e.preventDefault();
    await onsubmit();
  }}>
  <p>{@render children()}</p>
  <input
    aria-label={label}
    autocomplete="off"
    data-initial-focus
    onfocus={e => e.currentTarget.select()}
    {placeholder}
    spellcheck="false"
    bind:value
  />
  <code>{path}</code>
</form>

<style>
  .name-field {
    display: flex;
    flex-direction: column;
    gap: 10px;

    p {
      margin: 0;
    }

    input {
      padding-block: 9px;
      padding-inline: 12px;
      border: 1px solid var(--outline);
      border-radius: var(--radius-medium);
      background: var(--surface-2);
      color: var(--on-surface);
      font: inherit;
      font-weight: 600;
      font-size: 14px;

      &:focus-visible {
        border-color: var(--primary);
        outline: none;
      }
    }

    code {
      color: var(--on-surface-variant);
      font-family: var(--font-monospace);
      font-size: 11px;
      overflow-wrap: anywhere;
    }
  }
</style>
