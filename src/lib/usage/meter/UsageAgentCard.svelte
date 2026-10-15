<script lang="ts">
  import { AgentId } from "@/lib/agent/agent-icon";
  import Icon from "@/lib/Icon.svelte";
  import LimitResetOffer from "@/lib/usage/LimitResetOffer.svelte";
  import type { AgentGroup } from "@/lib/usage/usage-groups";
  import { groupSeverityClass, isJustReset } from "@/lib/usage/usage-meter";
  import UsageLimitRow from "@/lib/usage/UsageLimitRow.svelte";

  // One agent in the details panel: its name and plan, then a full-width bar row
  // per active limit.
  const { group, justReset }: {
    group: AgentGroup;
    justReset: ReadonlySet<string>;
  } = $props();

  const plan = $derived(group.plan || "usage not available locally");
</script>

<li class={["agent-card severity", groupSeverityClass(group)]} class:unknown={group.isUnknown}>
  <div class="agent-head">
    <span class="agent-icon"><Icon name={group.icon} /></span>
    <span class="agent-name">{group.name}</span>
    <span class="agent-plan">{plan}</span>
  </div>
  {#if group.limits.length > 0}
    <div class="limits">
      {#each group.limits as limit (limit.label)}
        <UsageLimitRow
          isJustReset={isJustReset({
            justReset,
            agentId: group.id,
            limit
          })}
          {limit}
        />
      {/each}
    </div>
  {/if}
  {#if group.id === AgentId.Claude}
    <LimitResetOffer />
  {/if}
</li>

<style>
  .agent-card {
    display: flex;
    flex-direction: column;
    gap: 9px;
    padding: 11px 12px;
    border: 1px solid var(--outline);
    border-radius: var(--radius-medium);
    background: var(--surface-1);

    /* Near/crit cards lift onto the severity wash with a matching hairline. */
    &.warning,
    &.critical {
      border-color: color-mix(in sRGB, var(--severity-color) 35%, var(--outline));
      background: var(--severity-wash);
    }

    /* An unknown agent has no severity - mute its icon so the primary blue
       never reads as a (fabricated) "healthy" score. */
    &.unknown {
      --severity-color: var(--on-surface-variant);
    }
  }

  /* Header: agent icon + name, with the plan trailing it, muted. */
  .agent-head {
    display: flex;
    gap: 7px;
    align-items: center;
    min-inline-size: 0;
  }

  .agent-name {
    overflow: hidden;
    font-weight: 700;
    font-size: 12.5px;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .agent-plan {
    overflow: hidden;
    color: var(--on-surface-variant);
    font-size: 10px;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  /* One shared grid across every limit row so the badge / bar / percent /
     reset columns line up down the card (each row is display: contents). */
  .limits {
    display: grid;
    grid-template-columns: auto 1fr auto auto;
    gap: 8px 9px;
    align-items: center;
  }
</style>
