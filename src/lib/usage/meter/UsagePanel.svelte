<script lang="ts">
  import { USAGE_MENU_ANCHOR, USAGE_MENU_ID, USAGE_PANEL_LABEL } from "@/lib/usage/meter/usage-meter";
  import UsageAgentCard from "@/lib/usage/meter/UsageAgentCard.svelte";
  import UsageKindLegend from "@/lib/usage/meter/UsageKindLegend.svelte";
  import UsageSpotlight from "@/lib/usage/meter/UsageSpotlight.svelte";
  import UsageSummary from "@/lib/usage/meter/UsageSummary.svelte";
  import { type AgentGroup, findSpotlight } from "@/lib/usage/usage-groups";

  // The meter's details popover: the summary, the limit closest to its cap, and
  // a card per running agent under a legend of the kind codes.
  const { groups, justReset }: {
    groups: AgentGroup[];
    justReset: ReadonlySet<string>;
  } = $props();

  const spotlight = $derived(findSpotlight(groups));
</script>

<div
  id={USAGE_MENU_ID}
  style:position-anchor={USAGE_MENU_ANCHOR}
  class="panel"
  aria-label={USAGE_PANEL_LABEL}
  popover
  role="dialog"
>
  {#if groups.length > 0}
    <UsageSummary {groups} />

    {#if spotlight}
      <UsageSpotlight {justReset} {spotlight} />
    {/if}

    <div class="agents-head">
      <span class="eyebrow">All agents</span>
      <UsageKindLegend {groups} />
    </div>

    <ul class="agents">
      {#each groups as group (group.id)}
        <UsageAgentCard {group} {justReset} />
      {/each}
    </ul>
  {:else}
    <p class="empty">No usage to show yet - sign in to Claude Code to load your limits</p>
  {/if}
</div>

<style>
  .panel {
    position: absolute;
    inset: auto;

    /* A native [popover] defaults to overflow:auto - clip so no stray scrollbar
       shows; the inner .agents list owns vertical scrolling. */
    overflow: clip;
    inline-size: min(29rem, 92vw);
    margin-block-start: 6px;
    padding: 15px;
    border: 1px solid var(--outline);
    border-radius: 16px;
    background: var(--surface-2);
    color: var(--on-surface);
    box-shadow: 0 16px 40px var(--shadow-color);
    position-area: bottom span-left;
    position-try-fallbacks: flip-block, flip-inline, flip-block flip-inline;
  }

  /* Shared by the panel's sections, at zero specificity (`:where`) so a
     section's own rule still wins. A small uppercase section label: */
  :where(.panel) :global(.eyebrow) {
    color: var(--on-surface-variant);
    font-weight: 700;
    font-size: 0.625rem;
    letter-spacing: 0.08em;
    text-transform: uppercase;
  }

  /* An agent's icon, tinted by whichever severity its context set on --severity-color. */
  :where(.panel) :global(.agent-icon) {
    display: inline-flex;
    flex: none;
    color: var(--severity-color);
    font-size: 0.9375rem;
  }

  /* ── "All agents" heading + kind-code legend ── */
  .agents-head {
    display: flex;
    gap: 8px;
    justify-content: space-between;
    align-items: center;
    margin-block-end: 8px;
  }

  /* ── Agent cards: a header, then one full-width bar row per active limit ── */
  .agents {
    display: flex;
    flex-direction: column;
    gap: 7px;
    overflow-y: auto;
    max-block-size: min(46vh, 22rem);
    margin: 0;
    padding: 0;
    list-style: none;
  }

  .empty {
    margin: 0;
    color: var(--on-surface-variant);
    font-size: 0.75rem;
    line-height: 1.5;
  }
</style>
