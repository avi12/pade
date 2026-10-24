<script lang="ts">
  import { AgentId } from "@/lib/agent/agent-icon";
  import Icon from "@/lib/Icon.svelte";
  import { growInline } from "@/lib/ui/motion";
  import { groupSeverityClass } from "@/lib/usage/meter/usage-meter";
  import UsageLimitStack from "@/lib/usage/meter/UsageLimitStack.svelte";
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
  <UsageLimitStack {group} {justReset} />
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

  .reset-slot {
    visibility: hidden;
    anchor-name: --reset-slot;
  }
</style>
