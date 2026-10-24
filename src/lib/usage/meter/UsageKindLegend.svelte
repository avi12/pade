<script lang="ts">
  import { type AgentGroup, buildKindLegend } from "@/lib/usage/usage-groups";

  // Decodes the mono kind codes the pills and rows use ("S" Session, "W"
  // Weekly, …) - only the codes actually in play.
  const { groups }: { groups: AgentGroup[] } = $props();

  const kindLegend = $derived(buildKindLegend(groups));
</script>

{#if kindLegend.length > 0}
  <span class="kind-legend">
    {#each kindLegend as entry (entry.short)}
      <span class="kind-legend-item">
        <span class="kind-code">{entry.short}</span>{entry.name}
      </span>
    {/each}
  </span>
{/if}

<style>
  .kind-legend {
    display: flex;
    flex-wrap: wrap;
    gap: 5px 10px;
    justify-content: flex-end;
  }

  .kind-legend-item {
    display: inline-flex;
    gap: 4px;
    align-items: center;
    color: var(--on-surface-variant);
    font-size: 0.5625rem;
  }

  /* The mono kind code (a small filled square). */
  .kind-code {
    display: inline-flex;
    flex: none;
    justify-content: center;
    align-items: center;
    block-size: 14px;
    inline-size: 14px;
    border-radius: 4px;
    background: var(--surface-3);
    color: var(--on-surface);
    font-family: var(--font-monospace);
    font-weight: 800;
    font-size: 0.5rem;
  }
</style>
