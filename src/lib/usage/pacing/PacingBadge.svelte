<script lang="ts">
  // A tab's usage-pacing badge: shown while pacing is holding the session back,
  // "PACED 2/4" - the active steps out of the allowed ones. It opens a popover
  // (PacingDetails) that says why, what it runs now and when it reverts, with
  // the overrides right there.

  import { pacingStatus } from "@/lib/stores/usage/usagePacing.svelte";
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
  <button
    style:anchor-name={anchorName}
    class="badge"
    aria-label={`Usage pacing: ${status.steps.length} of ${status.allowedCount} steps active`}
    data-noreorder
    data-tooltip="Usage pacing is holding this agent back - click for why"
    popovertarget={popoverTarget}
    type="button"
  >
    {#if status.isStopped}
      PAUSED
    {:else}
      PACED {status.steps.length}/{status.allowedCount}
    {/if}
  </button>

  {#if !isMeasuring}
    <div id={popoverId} style:position-anchor={anchorName} class="pacing-popover popover-menu" popover>
      <PacingDetails {sessionId} {status} />
    </div>
  {/if}
{/if}

<style>
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
