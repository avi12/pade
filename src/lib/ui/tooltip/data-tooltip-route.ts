// Every `data-tooltip` hint, shown through the one top-layer bubble
// (OverflowTooltipLayer) instead of a `::after` pseudo. A pseudo is a
// `position: fixed` box that escapes only as far as its containing block: a
// scroll container, a sticky row, a transformed or `content-visibility`
// ancestor or a [popover] clips it or paints over it. Only the top layer sits
// above everything, so routing every hint there is the one way none is ever
// clipped. One delegated listener on the document serves every trigger, so a
// component just writes `data-tooltip="…"` and never thinks about where it sits.

import { clearOverflowTooltip, showOverflowTooltip } from "@/lib/ui/tooltip/overflow-tooltip.svelte";
import { TRIGGER_ATTRIBUTE as ATTACHMENT_TRIGGER_ATTRIBUTE } from "@/lib/ui/tooltip/truncation-tooltip";

const TOOLTIP_ATTRIBUTE = "data-tooltip";
// A trigger of the `tooltip()` / `truncationTooltip()` attachments shows its
// own bubble; the innermost trigger of either kind speaks for the pointer.
const ANY_TRIGGER_SELECTOR = `[${TOOLTIP_ATTRIBUTE}], [${ATTACHMENT_TRIGGER_ATTRIBUTE}]`;
const ANCHOR_NAME = "--data-tooltip-trigger";
const ANCHOR_NAME_PROPERTY = "anchor-name";
const NO_ANCHOR = "none";
// While a control's menu or details are open its hints would sit under them:
// a dropdown trigger (and the hints inside it, like the usage pill's limits)
// while its `.menu-host` menu is open, and a tab's own controls while its
// pacing details are open. Bounded `:has()`, matched on one element per hover.
const QUIET_SELECTOR = [
  ".menu-host:has([popover]:popover-open) .menu-trigger:not([popover] *)",
  ".menu-host:has([popover]:popover-open) .menu-trigger:not([popover] *) *",
  "[data-session-tab]:has(.pacing-popover:popover-open) :not([popover] *)"
].join(", ");

interface Shown {
  elTrigger: HTMLElement;
  /** The trigger's own inline `anchor-name`, put back when the bubble goes. */
  inlineAnchorName: string;
  watcher: MutationObserver;
}

let shown: Shown | null = null;

function hide() {
  if (shown === null) {
    return;
  }

  const { elTrigger, inlineAnchorName, watcher } = shown;
  shown = null;
  watcher.disconnect();

  if (inlineAnchorName === "") {
    elTrigger.style.removeProperty(ANCHOR_NAME_PROPERTY);
  } else {
    elTrigger.style.setProperty(ANCHOR_NAME_PROPERTY, inlineAnchorName);
  }

  clearOverflowTooltip(ANCHOR_NAME);
}

function showFor(elTrigger: HTMLElement) {
  hide();
  const text = elTrigger.getAttribute(TOOLTIP_ATTRIBUTE) ?? "";
  if (text === "") {
    return;
  }

  // A trigger may already be an anchor (a button its popover hangs from), so
  // the bubble's name is added beside its own rather than replacing it.
  const inlineAnchorName = elTrigger.style.getPropertyValue(ANCHOR_NAME_PROPERTY);
  const ownAnchorName = getComputedStyle(elTrigger).getPropertyValue(ANCHOR_NAME_PROPERTY);
  const hasOwnAnchor = ownAnchorName !== "" && ownAnchorName !== NO_ANCHOR;
  elTrigger.style.setProperty(ANCHOR_NAME_PROPERTY, hasOwnAnchor ? `${ownAnchorName}, ${ANCHOR_NAME}` : ANCHOR_NAME);
  // A hint that changes while it shows (a timer, a state word) reads live.
  const watcher = new MutationObserver(() => {
    const isStillShown = shown?.elTrigger === elTrigger;
    const changedText = elTrigger.getAttribute(TOOLTIP_ATTRIBUTE) ?? "";
    if (!isStillShown) {
      return;
    }

    if (changedText === "") {
      hide();
      return;
    }

    showOverflowTooltip({
      anchorName: ANCHOR_NAME,
      text: changedText
    });
  });
  watcher.observe(elTrigger, { attributeFilter: [TOOLTIP_ATTRIBUTE] });
  shown = {
    elTrigger,
    inlineAnchorName,
    watcher
  };
  showOverflowTooltip({
    anchorName: ANCHOR_NAME,
    text
  });
}

/** The `data-tooltip` trigger that speaks for `target`, or null: none, a
 *  nearer attachment trigger that shows its own bubble, or a quiet one. */
function triggerFor(target: EventTarget | null) {
  if (!(target instanceof Element)) {
    return null;
  }

  const elTrigger = target.closest<HTMLElement>(ANY_TRIGGER_SELECTOR);
  const isOwnBubble = elTrigger === null || elTrigger.hasAttribute(ATTACHMENT_TRIGGER_ATTRIBUTE);
  if (isOwnBubble || elTrigger.matches(QUIET_SELECTOR)) {
    return null;
  }

  return elTrigger;
}

function follow(target: EventTarget | null) {
  const elTrigger = triggerFor(target);
  if (elTrigger === shown?.elTrigger) {
    return;
  }

  if (elTrigger === null) {
    hide();
    return;
  }

  showFor(elTrigger);
}

/** Route every `data-tooltip` in `document` through the top-layer bubble, from
 *  the one place that renders it. Returns the teardown. */
export function routeDataTooltips() {
  const controller = new AbortController();
  const { signal } = controller;
  addEventListener("pointerover", e => follow(e.target), { signal });
  // Leaving the window, or the page under the pointer changing beneath it.
  document.documentElement.addEventListener("pointerleave", hide, { signal });
  // Focus speaks only with a visible ring - a click's focus is the pointer's.
  addEventListener("focusin", e => {
    const isKeyboardFocus = e.target instanceof Element && e.target.matches(":focus-visible");
    if (isKeyboardFocus) {
      follow(e.target);
    }
  }, { signal });
  addEventListener("focusout", hide, { signal });
  // A scroll moves the trigger under a still bubble, as a native hint closes.
  addEventListener("scroll", hide, {
    capture: true,
    passive: true,
    signal
  });
  // A press acts on the control; its hint has said its piece.
  addEventListener("pointerdown", hide, {
    capture: true,
    signal
  });
  return () => {
    controller.abort();
    hide();
  };
}
