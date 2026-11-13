<script lang="ts">
  import { AgentId } from "@/lib/agent/agent-icon";
  import type { LimitReset } from "@/lib/types/usage";
  import type { AgentGroup } from "@/lib/usage/groups/usage-model";
  import {
    NO_READING,
    triggerPillGroups,
    USAGE_MENU_ANCHOR,
    USAGE_MENU_ID,
    usageAriaLabel
  } from "@/lib/usage/meter/usage-meter";
  import UsageAgentPill from "@/lib/usage/meter/UsageAgentPill.svelte";
  import UsageOverflowChip from "@/lib/usage/meter/UsageOverflowChip.svelte";

  // The meter's always-visible face: one pill per agent, with a mark under each
  // limit a free usage reset can clear, and a "+N" chip for the rest - all
  // opening the details panel, where the resets are spent.
  const { groups, justReset, resetOffers }: {
    groups: AgentGroup[];
    justReset: ReadonlySet<string>;
    /** Free usage resets the account holds, one entry per kind. */
    resetOffers: readonly LimitReset[];
  } = $props();
</script>

<button
  style:anchor-name={USAGE_MENU_ANCHOR}
  class="pill menu-trigger"
  aria-label={usageAriaLabel(groups)}
  data-tooltip="Usage by agent"
  popovertarget={USAGE_MENU_ID}
>
  {#if groups.length === 0}
    <span class="none">{NO_READING}</span>
  {:else}
    <span class="pills">
      {#each triggerPillGroups(groups) as group (group.id)}
        <UsageAgentPill
          {group}
          {justReset}
          resetOffers={group.id === AgentId.Claude ? resetOffers : []}
        />
      {/each}
      <UsageOverflowChip {groups} />
    </span>
  {/if}
  <span class="caret">▾</span>
</button>

<style>
  .pill {
    display: inline-flex;
    gap: 8px;
    align-items: center;
    padding-block: 4px;
    padding-inline: 11px 9px;
    border: none;
    border-radius: var(--radius-full);
    background: var(--surface-2);
    color: var(--on-surface);
    cursor: pointer;
    transition: background 150ms var(--ease);

    &:hover {
      background: var(--surface-3);
    }

    .none {
      color: var(--on-surface-variant);
      font-weight: 700;
      font-size: 0.75rem;
    }

    .pills {
      display: inline-flex;
      gap: 4px;
      align-items: center;
    }

    .caret {
      font-size: 0.625rem;
      opacity: 0.6;
    }
  }
</style>
