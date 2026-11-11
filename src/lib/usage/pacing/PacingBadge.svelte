<script lang="ts">
  // A tab's usage-pacing badge: shown while pacing is holding the session back -
  // SLOWED while it runs on a cheaper setup, PAUSED while it waits. It opens a popover
  // (PacingDetails) that says why, what it runs now and when it reverts, with
  // the overrides right there.

  import { pacingStatus } from "@/lib/stores/usage/usagePacing.svelte";
  import { tooltip } from "@/lib/ui/tooltip/truncation-tooltip";
  import PacingDetails from "@/lib/usage/pacing/PacingDetails.svelte";

  const { sessionId, isMeasuring = false }: {
    sessionId: string;
    /** In the tab strip's off-layout measuring copy: the badge takes its space,
     *  but never claims the popover. */
    isMeasuring?: boolean;
  } = $props();

  const status = $derived(pacingStatus(sessionId));
  const popoverId = $derived(`pacing-${sessionId}`);
  const anchorName = $derived(`--pacing-${sessionId}`);
  const popoverTarget = $derived.by(() => {
    if (isMeasuring) {
      return undefined;
    }

    return popoverId;
  });
</script>

{#if status}
  <!-- The hint takes the shared top-layer bubble: as a `data-tooltip` pseudo it
       was clipped by the tab strip, painted under the pacing note below it, and
       shaken by a paused badge's alarm animation. It sits on a wrapper because
       the bubble anchors itself through `anchor-name`, which the button already
       spends on its popover. -->
  <span class="badge-hint" {@attach tooltip("Slowed so your usage lasts until it resets - click for why")}>
    <button
      style:anchor-name={anchorName}
      class="badge"
      class:stopped={status.isStopped}
      data-noreorder
      popovertarget={popoverTarget}
      type="button"
    >
      {#if status.isStopped}
        PAUSED
      {:else}
        SLOWED
      {/if}
    </button>
  </span>

  {#if !isMeasuring}
    <div id={popoverId} style:position-anchor={anchorName} class="pacing-popover popover-menu" popover>
      <PacingDetails {sessionId} {status} />
    </div>
  {/if}
{/if}

<style>
  .badge-hint {
    display: inline-flex;
    flex: none;
  }

  .badge {
    flex: none;
    padding-block: 1px;
    padding-inline: 6px;
    border: 1px solid var(--warning);
    border-radius: var(--radius-full);
    background: var(--warning-wash);
    color: var(--warning);
    font-family: var(--font-monospace);
    font-weight: 700;
    font-size: 0.625rem;
    letter-spacing: 0.04em;

    /* Cyberpunk: a session pacing has stopped outright stutters every few
       seconds - an alarm on a held state, not on the step count changing. */
    @media (prefers-reduced-motion: no-preference) {
      :global(:root[data-palette="cyberpunk"]) &.stopped {
        animation: var(--cyber-alarm-animation);
      }
    }
  }

  .pacing-popover {
    display: flex;
    flex-direction: column;
    gap: 6px;
    inline-size: 280px;
    padding: 10px;
    color: var(--on-surface);
    font-size: 0.75rem;
    position-area: bottom span-right;
  }
</style>
