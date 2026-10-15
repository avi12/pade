<script generics="Value extends string" lang="ts">
  // A single-choice control, so it's a native radio group in a <fieldset>: each
  // option is a <label> wrapping a hidden-but-focusable radio, lit via :has. The
  // <legend> names the group for assistive tech only.
  const { legend, name, options, chosen, onchoose }: {
    legend: string;
    name: string;
    options: readonly {
      value: Value;
      label: string;
    }[];
    chosen: Value;
    onchoose: (value: Value) => void;
  } = $props();
</script>

<fieldset class="segmented-control">
  <legend class="visually-hidden">{legend}</legend>
  {#each options as option (option.value)}
    <label>
      <input
        {name}
        checked={chosen === option.value}
        onchange={() => onchoose(option.value)}
        type="radio"
      />{option.label}</label>
  {/each}
</fieldset>

<style>
  .segmented-control {
    display: flex;
    flex-shrink: 0;
    gap: 2px;
    margin: 0;
    padding: 3px;
    border: 0;
    border-radius: var(--radius-full);
    background: var(--surface-3);

    label {
      padding: 4px 11px;
      border-radius: var(--radius-full);
      color: var(--on-surface-variant);
      font-weight: 600;
      font-size: 11px;
      cursor: pointer;
      transition:
        background 120ms var(--ease),
        color 120ms var(--ease);
    }

    label:has(input:checked) {
      background: var(--primary-container);
      color: var(--on-primary-container);
    }

    label:has(input:focus-visible) {
      outline: 2px solid var(--primary);
      outline-offset: 1px;
    }
  }

  /* The radio drives state and stays keyboard-reachable, but the pill label is
     the visible affordance - so hide the native dot (and the legend, which has
     no visible affordance of its own) without removing either. */
  .segmented-control input,
  .visually-hidden {
    position: absolute;
    overflow: hidden;
    block-size: 1px;
    inline-size: 1px;
    margin: -1px;
    padding: 0;
    clip-path: inset(50%);
    white-space: nowrap;
  }
</style>
