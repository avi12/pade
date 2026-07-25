<script lang="ts">
  import { overflowTooltipState } from "@/lib/overflow-tooltip.svelte";

  // The bubble is a manual [popover] so it renders in the TOP LAYER — above the
  // workspace switcher's own popover menu (a plain fixed div would paint behind
  // it) — and is never clipped by a scroll/`content-visibility` ancestor. It
  // stays in the DOM and toggles open as a trigger claims/releases it; the
  // anchor just moves, so no open/close churn when hovering row to row.
  let bubble = $state<HTMLDivElement>();

  function syncPopoverToState() {
    const element = bubble;
    if (!element) {
      return;
    }

    const isOpen = element.matches(":popover-open");
    if (overflowTooltipState.active && !isOpen) {
      element.showPopover();
    } else if (!overflowTooltipState.active && isOpen) {
      element.hidePopover();
    }
  }

  $effect(syncPopoverToState);
</script>

<div
  bind:this={bubble}
  style:position-anchor={overflowTooltipState.active?.anchorName ?? "none"}
  class="overflow-tooltip"
  popover="manual"
  role="tooltip"
>{overflowTooltipState.active?.text ?? ""}</div>

<style>
  .overflow-tooltip {
    /* Reset the UA popover box, then position off the trigger's anchor. */
    inset: auto;
    margin: 0;
    border: 0;
    overflow: visible;

    position: fixed;
    inset-block-start: anchor(bottom);
    justify-self: anchor-center;
    z-index: 200;
    inline-size: max-content;
    max-inline-size: min(320px, 90vw);
    margin-block-start: 6px;
    padding: 4px 9px;
    border-radius: var(--radius-small);
    background: var(--surface-3);
    color: var(--on-surface);
    font-family: var(--font-ui);
    font-weight: 500;
    font-size: calc(0.6875 * var(--font-base));
    line-height: 1.35;

    /* A path has no spaces to break at, so let it wrap within the cap. */
    white-space: normal;
    overflow-wrap: anywhere;
    box-shadow: 0 6px 20px var(--shadow-color);
    pointer-events: none;

    /* Flip above the trigger near the viewport bottom — reuses theme.css's
       shared try so the flipped gap matches the [data-tooltip] bubble. */
    position-try-fallbacks: --tooltip-flip-up;
    animation: overflow-tooltip-fade 120ms var(--ease) both;
  }

  @keyframes overflow-tooltip-fade {
    from {
      opacity: 0%;
    }

    to {
      opacity: 100%;
    }
  }

  @media (prefers-reduced-motion: reduce) {
    .overflow-tooltip {
      animation: none;
    }
  }
</style>
