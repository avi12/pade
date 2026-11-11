<script lang="ts">
  import { routeDataTooltips } from "@/lib/ui/tooltip/data-tooltip-route";
  import { overflowTooltipState } from "@/lib/ui/tooltip/overflow-tooltip.svelte";
  import { onMount } from "svelte";

  const POPOVER_OPEN_SELECTOR = ":popover-open";

  // Every `data-tooltip` in the window shows through this bubble too.
  onMount(routeDataTooltips);

  // The bubble is a manual [popover] so it renders in the TOP LAYER - above the
  // workspace switcher's own popover menu (a plain fixed div would paint behind
  // it) - and is never clipped by a scroll/`content-visibility` ancestor. It
  // stays in the DOM and toggles open as a trigger claims/releases it; the
  // anchor just moves, so no open/close churn when hovering row to row.
  let elBubble = $state<HTMLParagraphElement>();

  $effect(() => {
    const isClaimed = overflowTooltipState.active !== null;
    const isInSync = elBubble?.matches(POPOVER_OPEN_SELECTOR) === isClaimed;
    if (!elBubble || isInSync) {
      return;
    }

    elBubble.togglePopover(isClaimed);
  });
</script>

<p
  bind:this={elBubble}
  style:position-anchor={overflowTooltipState.active?.anchorName ?? "none"}
  style:font-family={overflowTooltipState.active?.fontFamily}
  class="overflow-tooltip"
  popover="manual"
  role="tooltip"
>{overflowTooltipState.active?.text ?? ""}</p>

<style>
  /* The bubble's look, its frame included, is shared with the [data-tooltip]
     pseudo (theme.css); only its placement lives here. */
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
    animation: fadein 120ms var(--ease) both;

    /* Flip above the trigger near the viewport bottom - theme.css's shared
       try, keeping the gap/gutter tokens above. */
    position-try-fallbacks: --tooltip-flip-up;

    /* The moment its trigger lets go, the bubble loses its text and anchor
       while the shared popover exit would still be fading it; unanchored it
       dropped to the window's corner as an empty framed box, so a closing
       bubble is simply not drawn. */
    &:not(:popover-open) {
      visibility: hidden;
    }
  }

  @media (prefers-reduced-motion: reduce) {
    .overflow-tooltip {
      animation: none;
    }
  }
</style>
