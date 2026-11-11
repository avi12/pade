<script lang="ts">
  // A tab's usage-pacing badge: shown while pacing is holding the session back -
  // SLOWED while it runs on a cheaper setup, PAUSED while it waits. It opens a popover
  // (PacingDetails) that says why, what it runs now and when it reverts, with
  // the overrides right there.

  import { adaptedChoice } from "@/lib/stores/usage/usageAdapt.svelte";
  import { pacingStatus } from "@/lib/stores/usage/usagePacing.svelte";
  import { tooltip } from "@/lib/ui/tooltip/truncation-tooltip";
  import { pacingFactText, pacingResume } from "@/lib/usage/pacing/pacing-policy";
  import PacingDetails from "@/lib/usage/pacing/PacingDetails.svelte";

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
  <!-- The hint takes the shared top-layer bubble: as a `data-tooltip` pseudo it
       was clipped by the tab strip, painted under the pacing note below it, and
       shaken by a paused badge's alarm animation. It sits on a wrapper because
       the bubble anchors itself through `anchor-name`, which the button already
       spends on its popover. -->
  <span class="badge-hint" {@attach tooltip(() => hint)}>
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
    <!-- `data-noreorder`: the popover renders in the top layer but still sits
         inside the tab, so a press anywhere in it started dragging the tab. -->
    <div id={popoverId} style:position-anchor={anchorName} class="pacing-popover popover-menu" data-noreorder popover>
      <PacingDetails {resume} {sessionId} {status} />
    </div>
  {/if}
{/if}

<style>
  .badge-hint {
    display: inline-flex;
    flex: none;

    /* The details say more than the hint, so while they are open the hint
       goes: an anchored element is drawn only while its anchor is visible
       (position-visibility's default), and the shared bubble anchors on this
       wrapper - so the wrapper hides, and the badge inside shows again below,
       only then: an always-visible badge escaped the tab strip's hidden
       measuring copy, which showed as a second, draggable badge. */
    &:has(+ .pacing-popover:popover-open) {
      visibility: hidden;
    }
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

    /* Shown again inside its hidden wrapper (above). */
    .badge-hint:has(+ .pacing-popover:popover-open) & {
      visibility: visible;
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

    /* It sits inside the tab, whose grab hand would say it drags too. */
    cursor: auto;
    position-area: bottom span-right;
  }
</style>
