<script lang="ts">
  // A tab's usage-pacing badge: shown while pacing is holding the session back -
  // SLOWED while it runs on a cheaper setup, PAUSED while it waits. It opens a popover
  // (PacingDetails) that says why, what it runs now and when it reverts, with
  // the overrides right there.

  import { adaptedChoice } from "@/lib/stores/usage/usageAdapt.svelte";
  import { pacingStatus } from "@/lib/stores/usage/usagePacing.svelte";
  import PacingDetails from "@/lib/usage/pacing/badge/PacingDetails.svelte";
  import { pacingFactText, pacingResume } from "@/lib/usage/pacing/engine/pacing-policy";

  const { sessionId, isMeasuring = false }: {
    sessionId: string;
    /** In the tab strip's off-layout measuring copy: the badge takes its space,
     *  but never claims the popover. */
    isMeasuring?: boolean;
  } = $props();

  const status = $derived(pacingStatus(sessionId));
  const resume = $derived.by(() => {
    if (!status) {
      return null;
    }

    const choice = adaptedChoice(sessionId);
    const slowedUntil = status.revertsAt ?? choice?.restoreAt ?? status.verdict?.readyAt ?? null;
    return pacingResume({
      isStopped: status.isStopped,
      resumesAt: status.isStopped ? status.continuesAt : slowedUntil,
      original: choice?.original ?? null
    });
  });
  // Only when it is back to normal: the badge already says it is held, and a
  // click opens the why.
  const hint = $derived.by(() => {
    if (!resume) {
      return "";
    }

    return pacingFactText({
      fact: resume,
      now: Date.now()
    });
  });
  const popoverId = $derived(`pacing-${sessionId}`);
  // The measuring copy names no anchor: two badges sharing one name, the
  // popover anchored to the later one - the invisible copy - and was hidden
  // with it, so a click opened nothing that could be seen.
  const anchorName = $derived.by(() => {
    if (isMeasuring) {
      return undefined;
    }

    return `--pacing-${sessionId}`;
  });
  const popoverTarget = $derived.by(() => {
    if (isMeasuring) {
      return undefined;
    }

    return popoverId;
  });
</script>

{#if status && resume}
  <!-- While the details are open the tab's hints stay quiet (data-tooltip-route). -->
  <button
    style:anchor-name={anchorName}
    class="badge"
    data-noreorder
    data-tooltip={hint}
    popovertarget={popoverTarget}
    type="button"
  >
    {#if status.isStopped}
      PAUSED
    {:else}
      SLOWED
    {/if}
  </button>

  {#if !isMeasuring}
    <!-- `data-noreorder`: the popover renders in the top layer but still sits
         inside the tab, so a press anywhere in it started dragging the tab. -->
    <div id={popoverId} style:position-anchor={anchorName} class="pacing-popover popover-menu" data-noreorder popover>
      <PacingDetails {resume} {sessionId} {status} />
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

    /* It sits inside the tab, whose grab hand would say it drags too. */
    cursor: auto;
    position-area: bottom span-right;
  }
</style>
