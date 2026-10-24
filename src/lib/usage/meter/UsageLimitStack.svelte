<script lang="ts">
  import { growInline } from "@/lib/ui/motion";
  import { isJustReset, NO_READING, severityClass } from "@/lib/usage/meter/usage-meter";
  import type { AgentGroup } from "@/lib/usage/usage-groups";

  // One agent's limits as compact stacks - the mono kind letter over its
  // percent, coloured by how close it is - or a dash with no reading. Shared by
  // the top bar's agent pill and the composer's LLM menu, so they read alike.
  const { group, justReset = new Set() }: {
    group: AgentGroup;
    /** Limits that just started over, whose numbers flash. */
    justReset?: ReadonlySet<string>;
  } = $props();
</script>

{#if group.limits.length > 0}
  <span class="limits">
    {#each group.limits as limit (limit.label)}
      <span
        class={["limit severity", severityClass(limit.level)]}
        class:just-reset={isJustReset({
          justReset,
          agentId: group.id,
          limit
        })}
        transition:growInline
      >
        <span class="kind">{limit.kindShort}</span>
        <span class="percent">{limit.reading}</span>
      </span>
    {/each}
  </span>
{:else}
  <span class="dash">{NO_READING}</span>
{/if}

<style>
  .limits {
    display: inline-flex;
    gap: 8px;
    align-items: stretch;
  }

  .limit {
    display: inline-flex;
    flex-direction: column;
    gap: 1px;
    align-items: center;
    border-radius: var(--radius-small);
    line-height: 1;
    white-space: nowrap;

    .kind {
      color: var(--on-surface-variant);
      font-family: var(--font-monospace);
      font-weight: 800;
      font-size: 0.5rem;
      letter-spacing: 0.02em;
    }

    .percent {
      color: var(--severity-color);
      font-weight: 700;
      font-size: 0.75rem;
    }
  }

  .dash {
    color: var(--on-surface-variant);
    font-weight: 700;
    font-size: 0.6875rem;
  }
</style>
