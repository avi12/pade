<script generics="Value extends string | boolean | undefined" lang="ts">
  import Icon, { type IconName } from "@/lib/Icon.svelte";

  // A single-select row of options (buttons + aria-pressed). One row of equal
  // options by default; a parent lays them out otherwise through
  // `--segmented-columns` (a grid-template-columns value) on an ancestor.
  const { label, options, chosen, onchoose }: {
    label: string;
    options: readonly {
      value: Value;
      label: string;
      icon?: IconName;
    }[];
    chosen: Value;
    onchoose: (value: Value) => void;
  } = $props();
</script>

<div class="segmented" aria-label={label} role="group">
  {#each options as option (option.label)}
    <button
      class="option"
      class:on={chosen === option.value}
      aria-pressed={chosen === option.value}
      onclick={() => onchoose(option.value)}
      type="button"
    >
      {#if option.icon}
        <Icon name={option.icon} />
      {/if}
      <span>{option.label}</span>
    </button>
  {/each}
</div>

<style>
  .segmented {
    display: grid;
    grid-template-columns: var(--segmented-columns, none);
    grid-auto-columns: 1fr;
    grid-auto-flow: var(--segmented-flow, column);
    gap: 4px;
    padding: 4px;
    border-radius: 12px;
    background: var(--surface-2);
  }

  .option {
    display: inline-flex;
    gap: 6px;
    justify-content: center;
    align-items: center;
    padding: 7px 8px;
    border: none;
    border-radius: 9px;
    background: transparent;
    color: var(--on-surface-variant);
    font: inherit;
    font-weight: 600;
    font-size: 0.75rem;
    cursor: pointer;
    transition:
      background 200ms var(--ease),
      color 200ms var(--ease);

    &:hover:not(.on) {
      background: var(--surface-3);
      color: var(--on-surface);
    }
  }

  .on {
    background: var(--primary-container);
    color: var(--on-primary-container);
  }
</style>
