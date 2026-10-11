<script lang="ts">
  // An effort level as rising bars — `step` of `steps` lit, each bar taller than
  // the last, so "more thinking" reads at a glance the way signal strength does.
  // Purely visual: the caller carries the words in its tooltip / label.
  const { step, steps }: {
    step: number;
    steps: number;
  } = $props();

  const rungs = $derived(Array.from({ length: steps }, (_, index) => index + 1));
</script>

<span style:--steps={steps} class="effort-bars" aria-hidden="true">
  {#each rungs as rung (rung)}
    <span style:--rung={rung} class="rung" class:lit={rung <= step}></span>
  {/each}
</span>

<style>
  .effort-bars {
    display: inline-flex;
    flex: none;
    gap: 1.5px;
    align-items: flex-end;
    block-size: 11px;

    .rung {
      block-size: calc(100% * var(--rung) / var(--steps));
      min-block-size: 3px;
      inline-size: 3px;
      background: currentColor;
      opacity: 22%;
      transition:
        opacity 140ms var(--ease),
        background-color 140ms var(--ease);

      &.lit {
        background: var(--effort-lit, currentColor);
        opacity: 100%;
      }
    }
  }
</style>
