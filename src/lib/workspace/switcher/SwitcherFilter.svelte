<script lang="ts">
  import Icon from "@/lib/Icon.svelte";
  import { Key } from "@/lib/ui/keyboard/keys";

  // The left rail's filter / quick-switch field: typing narrows the project
  // rows, Enter jumps to the first one left.
  let { id, value = $bindable(), onenter }: {
    /** The field's id - Ctrl P focuses it by this. */
    id: string;
    value: string;
    /** Enter was pressed in the field. */
    onenter: () => void;
  } = $props();
</script>

<label class="search text-field-plate">
  <span class="lead" aria-hidden="true"><Icon name="search" size={15} /></span>
  <input
    {id}
    aria-label="Switch project by name or path"
    autocomplete="off"
    onkeydown={e => {
      if (e.key === Key.Enter) {
        onenter();
      }
    }}
    placeholder="Search projects…"
    spellcheck="false"
    type="search"
    bind:value
  />
  <span class="kbd" aria-hidden="true">Ctrl P</span>
</label>

<style>
  .search {
    display: flex;
    gap: 8px;
    align-items: center;
    margin-block: 1px 5px;
    margin-inline: 2px;

    .lead {
      display: inline-flex;
      flex: none;
      color: var(--on-surface-variant);
    }

    input {
      flex: 1;
      min-inline-size: 0;
    }

    .kbd {
      flex: none;
      padding-block: 2px;
      padding-inline: 6px;
      border-radius: 6px;
      background: var(--surface-3);
      color: var(--on-surface-variant);
      font-family: var(--font-monospace);
      font-weight: 700;
      font-size: 0.5625rem;
    }
  }
</style>
