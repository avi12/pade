<script lang="ts">
  import { overflowTooltipState } from "@/lib/ui/tooltip/overflow-tooltip.svelte";

  const POPOVER_OPEN_SELECTOR = ":popover-open";

  // The bubble is a manual [popover] so it renders in the TOP LAYER - above the
  // workspace switcher's own popover menu (a plain fixed div would paint behind
  // it) - and is never clipped by a scroll/`content-visibility` ancestor. It
  // stays in the DOM and toggles open as a trigger claims/releases it; the
  // anchor just moves, so no open/close churn when hovering row to row.
  let elBubble = $state<HTMLDivElement>();

  $effect(() => {
    const isClaimed = overflowTooltipState.active !== null;
    const isInSync = elBubble?.matches(POPOVER_OPEN_SELECTOR) === isClaimed;
    if (!elBubble || isInSync) {
      return;
    }

    elBubble.togglePopover(isClaimed);
  });
</script>

<div
  bind:this={elBubble}
  style:position-anchor={overflowTooltipState.active?.anchorName ?? "none"}
  style:font-family={overflowTooltipState.active?.fontFamily}
  class="overflow-tooltip"
  popover="manual"
  role="tooltip"
>{overflowTooltipState.active?.text ?? ""}</div>

<style>
  /* The bubble's look is shared with the [data-tooltip] pseudo (theme.css);
     only its placement lives here. */
  .overflow-tooltip {
    position: fixed;

    /* Reset the UA popover box, then position off the trigger's anchor. */
    inset: auto;
    inset-block-start: anchor(bottom);
    z-index: 200;
    justify-self: anchor-center;
    overflow: visible;
    margin-block: var(--tooltip-gap) 0;
    margin-inline: var(--tooltip-edge-gutter);
    border: 0;
    animation: fadein 120ms var(--ease) both;

    /* Flip above the trigger near the viewport bottom - reuses theme.css's
       shared try, and its gap/gutter tokens above, so this bubble and the
       [data-tooltip] pseudo are placed identically. */
    position-try-fallbacks: --tooltip-flip-up;
  }

  @media (prefers-reduced-motion: reduce) {
    .overflow-tooltip {
      animation: none;
    }
  }
</style>
