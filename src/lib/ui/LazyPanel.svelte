<script generics="T" lang="ts">
  import type { Snippet } from "svelte";

  // A side panel loaded only when it is first opened, so an unopened panel never
  // reaches the bundle. The caller renders the loaded component itself, which
  // keeps its props typed.
  const { load, children }: {
    load: () => Promise<{ default: T }>;
    children: Snippet<[T]>;
  } = $props();

  const loading = $derived(load());
</script>

{#await loading then { default: loaded }}
  {@render children(loaded)}
{:catch}
  <p class="panel-load-error">Could not load this panel. Reload the window to retry</p>
{/await}

<style>
  .panel-load-error {
    margin: 16px;
    color: var(--on-surface-variant);
    font-size: 0.8125rem;
    line-height: 1.5;
  }
</style>
