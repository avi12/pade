<script lang="ts">
  import {
    NO_READING,
    triggerPillGroups,
    USAGE_MENU_ANCHOR,
    USAGE_MENU_ID,
    usageAriaLabel
  } from "@/lib/usage/meter/usage-meter";
  import UsageAgentPill from "@/lib/usage/meter/UsageAgentPill.svelte";
  import UsageOverflowChip from "@/lib/usage/meter/UsageOverflowChip.svelte";
  import type { AgentGroup } from "@/lib/usage/usage-groups";
  import type { Snippet } from "svelte";

  // The meter's always-visible face: one pill per agent and a "+N" chip for the
  // rest, opening the details panel.
  const { groups, justReset, showsResetInPill, resetLabel }: {
    groups: AgentGroup[];
    justReset: ReadonlySet<string>;
    /** Claude's pill keeps a slot the free-reset chip is anchored over. */
    showsResetInPill: boolean;
    resetLabel: Snippet;
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
        <UsageAgentPill {group} {justReset} {resetLabel} {showsResetInPill} />
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
      font-size: 12px;
    }

    .pills {
      display: inline-flex;
      gap: 4px;
      align-items: center;
    }

    .caret {
      font-size: 10px;
      opacity: 60%;
    }
  }
</style>
