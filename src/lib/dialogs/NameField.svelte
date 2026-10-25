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
    /** What the name changes - shown above the field. */
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
    class="text-field"
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
      font-weight: 600;
    }

    code {
      color: var(--on-surface-variant);
      font-family: var(--font-monospace);
      font-size: 0.6875rem;
      overflow-wrap: anywhere;
    }
  }
</style>
