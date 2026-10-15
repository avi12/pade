<script generics="Value extends string" lang="ts">
  // The shared dropdown: a customizable select, so its open list wears the theme
  // too - the native one draws a plain system list. `--select-width` sets its
  // width (180px unless the parent says), so pickers down a card line up.
  const { id, value, options, onchange }: {
    /** For the `<label for>` that names it. */
    id: string;
    value: Value;
    options: readonly {
      value: Value;
      label: string;
    }[];
    onchange: (value: Value) => void;
  } = $props();
</script>

<select
  {id}
  onchange={e => {
    const picked = options.find(option => option.value === e.currentTarget.value);
    if (picked !== undefined) {
      onchange(picked.value);
    }
  }}
  {value}
>
  {#each options as option (option.value)}
    <option value={option.value}>{option.label}</option>
  {/each}
</select>

<style>
  select {
    appearance: base-select;
    flex: none;
    inline-size: var(--select-width, 180px);
    max-inline-size: 60%;
    padding-block: 5px;
    padding-inline: 10px;
    border: 1px solid var(--outline);
    border-radius: var(--radius-small);
    background: var(--surface-2);
    color: var(--on-surface);
    font: inherit;
    font-size: 12px;
    cursor: pointer;
    transition: border-color 140ms var(--ease);

    &:hover {
      border-color: var(--primary);
    }

    &::picker-icon {
      color: var(--on-surface-variant);
    }

    &::picker(select) {
      appearance: base-select;
      padding: 4px;
      border: 1px solid var(--outline);
      border-radius: var(--radius-small);
      background: var(--surface-3);
      color: var(--on-surface);
      box-shadow: 0 10px 30px var(--shadow-color);
    }
  }

  option {
    padding-block: 5px;
    padding-inline: 8px;
    border-radius: var(--radius-small);
    font-size: 12px;

    &:hover,
    &:focus-visible {
      background: var(--surface-2);
    }

    &:checked {
      background: var(--primary-container);
      color: var(--on-primary-container);
    }
  }
</style>
