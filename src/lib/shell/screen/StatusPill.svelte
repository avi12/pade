<script lang="ts">
  import type { Snippet } from "svelte";

  // A calm status pill with a pulsing dot - what an automatic recovery or a
  // usage pause shows while it works.
  const { children }: {
    children: Snippet;
  } = $props();
</script>

<output class="status-pill">
  <span class="status-dot"></span>
  {@render children()}
</output>

<style>
  .status-pill {
    display: inline-flex;
    gap: 9px;
    align-items: center;
    max-inline-size: min(560px, 90vw);
    padding: 10px 18px;
    border-radius: var(--radius-full);
    background: var(--primary-container);
    color: var(--on-primary-container);
    font-weight: 600;
    font-size: 13px;
    box-shadow: 0 10px 30px var(--shadow-color);
    animation: pop-in 220ms var(--ease);
  }

  .status-dot {
    flex: none;
    block-size: 8px;
    inline-size: 8px;
    border-radius: var(--radius-full);
    background: var(--primary);
    animation: pulse 1100ms var(--ease) infinite;
  }
</style>
