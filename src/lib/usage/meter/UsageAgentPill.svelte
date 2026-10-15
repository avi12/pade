<script lang="ts">
  import { AgentId } from "@/lib/agent/agent-icon";
  import Icon from "@/lib/Icon.svelte";
  import { growInline } from "@/lib/ui/motion";
  import { groupSeverityClass, isJustReset, NO_READING, severityClass } from "@/lib/usage/meter/usage-meter";
  import type { AgentGroup } from "@/lib/usage/usage-groups";
  import type { Snippet } from "svelte";

  // One agent in the trigger: icon + short name, then a stack per limit (the
  // mono kind letter over its percent).
  const { group, justReset, showsResetInPill, resetLabel }: {
    group: AgentGroup;
    justReset: ReadonlySet<string>;
    showsResetInPill: boolean;
    resetLabel: Snippet;
  } = $props();

  // The trigger is a button and can't hold the free-reset chip, so Claude's pill
  // keeps an invisible slot of the chip's exact size for the meter to anchor
  // the real chip over.
  const showsResetSlot = $derived(group.id === AgentId.Claude && showsResetInPill);
</script>

<span class={["agent-pill severity", groupSeverityClass(group)]} class:unknown={group.isUnknown}>
  <span class="agent-pill-id">
    <span class="agent-icon"><Icon name={group.icon} /></span>
    <span class="agent-pill-name">{group.shortName}</span>
  </span>
  {#if group.limits.length > 0}
    <span class="agent-pill-limits">
      {#each group.limits as limit (limit.label)}
        <span
          class={["agent-pill-limit severity", severityClass(limit.level)]}
          class:just-reset={isJustReset({
            justReset,
            agentId: group.id,
            limit
          })}
          transition:growInline
        >
          <span class="agent-pill-kind">{limit.kindShort}</span>
          <span class="agent-pill-percent">{limit.reading}</span>
        </span>
      {/each}
    </span>
  {:else}
    <span class="agent-pill-dash">{NO_READING}</span>
  {/if}
  {#if showsResetSlot}
    <span class="reset-face reset-slot" aria-hidden="true" transition:growInline>{@render resetLabel()}</span>
  {/if}
</span>

<style>
  .agent-pill {
    display: inline-flex;
    gap: 8px;
    align-items: center;
    padding-block: 4px;
    padding-inline: 7px 8px;
    border-radius: var(--radius-small);
    background: var(--surface-1);

    /* No trustworthy usage → mute the whole pill; the dash carries the meaning. */
    &.unknown {
      --severity-color: var(--on-surface-variant);
    }
  }

  .agent-icon {
    display: inline-flex;
    flex: none;
    color: var(--severity-color);
    font-size: 13px;
  }

  .agent-pill-id {
    display: inline-flex;
    flex: none;
    gap: 6px;
    align-items: center;
  }

  .agent-pill-name {
    overflow: hidden;
    max-inline-size: 4rem;
    color: var(--on-surface);
    font-weight: 600;
    font-size: 11px;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .agent-pill-limits {
    display: inline-flex;
    gap: 8px;
    align-items: stretch;
  }

  .agent-pill-limit {
    display: inline-flex;
    flex-direction: column;
    gap: 1px;
    align-items: center;
    border-radius: var(--radius-small);
    line-height: 1;
    white-space: nowrap;

    .agent-pill-kind {
      color: var(--on-surface-variant);
      font-family: var(--font-monospace);
      font-weight: 800;
      font-size: 8px;
      letter-spacing: 0.02em;
    }

    .agent-pill-percent {
      color: var(--severity-color);
      font-weight: 700;
      font-size: 12px;
    }
  }

  .agent-pill-dash {
    color: var(--on-surface-variant);
    font-weight: 700;
    font-size: 11px;
  }

  .reset-slot {
    visibility: hidden;
    anchor-name: --reset-slot;
  }
</style>
