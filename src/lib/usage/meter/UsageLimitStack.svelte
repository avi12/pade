<script lang="ts">
  import Icon from "@/lib/Icon.svelte";
  import type { LimitReset } from "@/lib/types/usage";
  import { growInline } from "@/lib/ui/motion";
  import type { AgentGroup } from "@/lib/usage/groups/usage-model";
  import { limitResetsFor } from "@/lib/usage/limit-reset/limit-reset-kind";
  import { formatCount } from "@/lib/util/format";
  import {
    isJustReset,
    limitTooltip,
    NO_READING,
    noReadingFace,
    severityClass
  } from "@/lib/usage/meter/usage-meter";

  // One agent's limits as compact stacks - the mono kind letter over its
  // percent, coloured by how close it is - or a dash with no reading. Shared by
  // the top bar's agent pill and the composer's LLM menu, so they read alike.
  const { group, justReset = new Set(), resetOffers = [] }: {
    group: AgentGroup;
    /** Limits that just started over, whose numbers flash. */
    justReset?: ReadonlySet<string>;
    /** The account's free resets: each limit one clears carries a mark. */
    resetOffers?: readonly LimitReset[];
  } = $props();

  const face = $derived(noReadingFace(group));
</script>

{#if group.limits.length > 0}
  <span class="limits">
    {#each group.limits as limit (limit.label)}
      {@const resets = limitResetsFor({
        offers: resetOffers,
        limit
      })}
      <!-- The letter alone doesn't say which limit it is: the hint names it. -->
      <span
        class={["limit severity", severityClass(limit.level)]}
        class:just-reset={isJustReset({
          justReset,
          agentId: group.id,
          limit
        })}
        data-tooltip={limitTooltip(limit)}
        transition:growInline
      >
        <span class="heading">
          <span class="kind">{limit.kindShort}</span>
          {#if resets.count > 0}
            <span class="reset" data-tooltip="{resets.description} available - open to use one">
              <Icon name="refresh" size={10} />{formatCount(resets.count)}
            </span>
          {/if}
        </span>
        <span class="percent">{limit.reading}</span>
      </span>
    {/each}
  </span>
{:else if face}
  <!-- Why there is no reading, and the fix on hover. -->
  <span
    class="dash"
    class:state={face.isState}
    data-tooltip={face.hint}
  >{face.label}</span>
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

    /* The label row carries the limit's letter and, beside it, the free resets
       that clear it - so a reset adds width, never a third row of height. */
    .heading {
      display: inline-flex;
      gap: 3px;
      align-items: center;
    }

    .kind {
      color: var(--on-surface-variant);
      font-family: var(--font-monospace);
      font-weight: 800;
      font-size: 0.625rem;
      letter-spacing: 0.02em;
    }

    .percent {
      color: var(--severity-color);
      font-weight: 700;
      font-size: 0.75rem;
    }

    .reset {
      display: inline-flex;
      gap: 2px;
      align-items: center;
      color: var(--primary);
      font-family: var(--font-monospace);
      font-weight: 800;
      font-size: 0.625rem;
    }
  }

  .dash {
    color: var(--on-surface-variant);
    font-weight: 700;
    font-size: 0.6875rem;
    white-space: nowrap;

    /* A named state ("Run to refresh") asks for action: it reads in the
       warning colour, quieter than a limit's number. */
    &.state {
      color: var(--warning);
      font-weight: 600;
      font-size: 0.625rem;
    }
  }
</style>
