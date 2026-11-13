<script lang="ts">
  import Icon from "@/lib/Icon.svelte";
  import type { LimitReset } from "@/lib/types/usage";
  import type { AgentGroup } from "@/lib/usage/groups/usage-model";
  import { groupSeverityClass } from "@/lib/usage/meter/usage-meter";
  import UsageLimitStack from "@/lib/usage/meter/UsageLimitStack.svelte";

  // One agent in the trigger: icon + short name, then a stack per limit (the
  // mono kind letter over its percent).
  const { group, justReset, resetOffers }: {
    group: AgentGroup;
    justReset: ReadonlySet<string>;
    resetOffers: readonly LimitReset[];
  } = $props();
</script>

<span class={["agent-pill severity", groupSeverityClass(group)]} class:unknown={group.isUnknown}>
  <span class="agent-pill-id">
    <span class="agent-icon"><Icon name={group.icon} /></span>
    <span class="agent-pill-name">{group.shortName}</span>
  </span>
  <UsageLimitStack {group} {justReset} {resetOffers} />
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
    font-size: 0.8125rem;
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
    font-size: 0.6875rem;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
</style>
