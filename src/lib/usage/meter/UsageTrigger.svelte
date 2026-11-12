<script lang="ts">
  import Icon from "@/lib/Icon.svelte";
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
  import { formatCount } from "@/lib/util/format";

  // The meter's always-visible face: one pill per agent, a "+N" chip for the
  // rest, and how many free usage resets wait - all opening the details panel,
  // where the resets are spent.
  const { groups, justReset, resetCount }: {
    groups: AgentGroup[];
    justReset: ReadonlySet<string>;
    /** Free usage resets the account holds, across their kinds. */
    resetCount: number;
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
        <UsageAgentPill {group} {justReset} />
      {/each}
      <UsageOverflowChip {groups} />
    </span>
  {/if}
  {#if resetCount > 0}
    <span class="reset-count" data-tooltip="{formatCount(resetCount)} usage resets available - open to use one">
      <Icon name="refresh" size={11} />{formatCount(resetCount)}
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
      opacity: 60%;
    }
  }

  /* Free resets waiting to be spent. Worth knowing about the moment one lands:
     as the count appears it pops, then a ring pulses out from it a few times -
     a CSS animation runs on insertion, so this plays exactly then. */
  .reset-count {
    display: inline-flex;
    gap: 3px;
    align-items: center;
    padding-block: 1px;
    padding-inline: 6px;
    border: 1px solid var(--primary);
    border-radius: var(--radius-full);
    background: color-mix(in oklch, var(--primary) 16%, transparent);
    color: var(--primary);
    font-weight: 700;
    font-size: 0.6563rem;
    line-height: 1.4;
    white-space: nowrap;
    animation: reset-arrive 1200ms var(--ease) 3;
  }

  @keyframes reset-arrive {
    0% {
      box-shadow: 0 0 0 0 color-mix(in oklch, var(--primary) 70%, transparent);
      scale: 1;
    }

    20% {
      scale: 1.15;
    }

    45% {
      scale: 1;
    }

    100% {
      box-shadow: 0 0 0 10px transparent;
      scale: 1;
    }
  }

  @media (prefers-reduced-motion: reduce) {
    .reset-count {
      animation: none;
    }
  }
</style>
