import { clearOverflowTooltip, showOverflowTooltip } from "@/lib/ui/tooltip/overflow-tooltip.svelte";

/** A tooltip text, or a thunk for one that must stay live (e.g. it names the
 *  currently ranked editor, which can change after the attachment mounts). */
type TooltipText = string | (() => string);

function resolve(text: TooltipText) {
  return typeof text === "function" ? text() : text;
}

// Unique anchor id per attached trigger, so the shared top-level bubble
// (OverflowTooltipLayer) can position itself back onto whichever one is hovered.
let nextAnchorId = 0;

const ANCHOR_NAME_PROPERTY = "anchor-name";
/** Marks a trigger of these attachments - it shows its own bubble. */
export const TRIGGER_ATTRIBUTE = "data-overflow-tooltip";
const NESTED_TOOLTIP_SELECTOR = `[data-tooltip], [${TRIGGER_ATTRIBUTE}]`;
interface Bubble {
  text: string;
  fontFamily?: string;
}

const NO_BUBBLE: Bubble = { text: "" };

/** A descendant with its own tooltip - a `[data-tooltip]` (a chat row's pause
 *  mark) or a trigger of this same bubble (a language bar segment) - speaks
 *  for itself, so over it the trigger's bubble steps aside and the two never
 *  stack. */
function isOverNestedTooltip({
  elTrigger,
  target
}: {
  elTrigger: HTMLElement;
  target: EventTarget | null | undefined;
}) {
  if (!(target instanceof Element)) {
    return false;
  }

  const elTooltipOwner = target.closest(NESTED_TOOLTIP_SELECTOR);
  return elTooltipOwner !== elTrigger && elTrigger.contains(elTooltipOwner);
}

/** The attachment's plumbing: name the trigger as an anchor, then show/hide
 *  the single top-layer bubble on hover/focus. `bubbleToShow` returns the text
 *  for the current state (empty string = no bubble). A hint that is always on
 *  is a plain `data-tooltip` (data-tooltip-route), shown through the same
 *  bubble. */
function overflowTooltip(bubbleToShow: (elTrigger: HTMLElement) => Bubble) {
  return (elTrigger: HTMLElement) => {
    const anchorName = `--overflow-tooltip-${nextAnchorId++}`;
    elTrigger.style.setProperty(ANCHOR_NAME_PROPERTY, anchorName);
    elTrigger.toggleAttribute(TRIGGER_ATTRIBUTE, true);

    function activate(e?: Event) {
      const { text, fontFamily } = isOverNestedTooltip({
        elTrigger,
        target: e?.target
      })
        ? NO_BUBBLE
        : bubbleToShow(elTrigger);
      if (!text) {
        deactivate();
        return;
      }

      showOverflowTooltip({
        anchorName,
        text,
        fontFamily
      });
      // Any scroll dismisses the bubble, as a native tooltip does: its row
      // may slide under a pinned section (which `anchors-visible` can't see,
      // since nothing clips it) while the bubble would still be riding it.
      addEventListener("scroll", deactivate, {
        capture: true,
        passive: true
      });
    }

    function deactivate() {
      removeEventListener("scroll", deactivate, { capture: true });
      clearOverflowTooltip(anchorName);
    }

    // Focus speaks only when the browser shows its ring - keyboard focus. A
    // menu opened by click still focuses its first row, and that row's bubble
    // would pop up under a cursor that is somewhere else.
    function activateOnVisibleFocus() {
      const isFocusVisible = elTrigger.matches(":focus-visible, :has(:focus-visible)");
      if (isFocusVisible) {
        activate();
      }
    }

    // `pointerover`, not `pointerenter`: it fires on every descendant crossing,
    // so the bubble steps aside over a nested tooltip and comes back after.
    elTrigger.addEventListener("pointerover", activate);
    elTrigger.addEventListener("pointerleave", deactivate);
    elTrigger.addEventListener("focusin", activateOnVisibleFocus);
    elTrigger.addEventListener("focusout", deactivate);
    return () => {
      deactivate();
      elTrigger.removeEventListener("pointerover", activate);
      elTrigger.removeEventListener("pointerleave", deactivate);
      elTrigger.removeEventListener("focusin", activateOnVisibleFocus);
      elTrigger.removeEventListener("focusout", deactivate);
      elTrigger.style.removeProperty(ANCHOR_NAME_PROPERTY);
      elTrigger.removeAttribute(TRIGGER_ATTRIBUTE);
    };
  };
}

/** Attachment that surfaces `tooltip` only while the element's text is actually
 *  clipped (ellipsized) - an already-readable line never grows a redundant
 *  bubble. `restingTooltip`, when given, is what the unclipped state shows
 *  instead (an action hint like "Open in WebStorm" stays useful without the
 *  redundant path); omitted, the unclipped state has no tooltip.
 *  `measureSelector` points at the descendants that actually clip when they
 *  differ from the hover host (a button whose inner spans clip); any one of
 *  them clipped counts. Clipped means cut short either way: an ellipsized line
 *  or a line-clamped block.
 *
 *  The clip check runs on hover (`pointerover`) - the one moment that both guarantees a
 *  settled layout (a mount-time measure can read a mid-layout width and brand an
 *  untruncated line clipped forever) and precedes the bubble showing. With real
 *  layout in hand the DOM's own overflow answer is exact, and re-checking per
 *  hover tracks panel resizes for free. */
export function truncationTooltip({
  tooltip: text,
  restingTooltip,
  measureSelector
}: {
  tooltip: TooltipText;
  restingTooltip?: TooltipText;
  measureSelector?: string;
}) {
  return overflowTooltip(elTrigger => {
    const elsMeasured = measureSelector ? [...elTrigger.querySelectorAll(measureSelector)] : [elTrigger];
    const elClipped = elsMeasured.find(
      elMeasured => elMeasured.scrollWidth > elMeasured.clientWidth || elMeasured.scrollHeight > elMeasured.clientHeight
    );
    if (elClipped) {
      // The bubble finishes the clipped line, so it reads in that line's face.
      return {
        text: resolve(text),
        fontFamily: getComputedStyle(elClipped).fontFamily
      };
    }

    // An empty resting text (e.g. "no editor ranked yet") means no bubble.
    return { text: restingTooltip === undefined ? "" : resolve(restingTooltip) };
  });
}
