// Global tooltip controller — one authoritative home (DRY) for every element
// annotated with `data-tooltip="…"`. Instead of a per-element `::after` bubble
// (which an `overflow:hidden` toolbar or a stacking context can clip), a single
// element is rendered through the **popover API** so it lives in the top layer
// and can never be clipped, and **CSS anchor positioning** pins it to whichever
// control is hovered or focused. Components keep using the `data-tooltip`
// attribute; this wires the behavior once from main.ts.
//
// A control may add `data-tooltip-pos="top"` to prefer opening above; either
// way the CSS `position-try-fallbacks` still flips it to stay on-screen.

const ANCHOR_NAME = "--app-tooltip-anchor";
const SHOW_DELAY_MS = 150;

let bubble: HTMLDivElement | null = null;
// The control the bubble is currently anchored to (its `anchor-name` is set).
let anchored: HTMLElement | null = null;
// The control the pointer/focus is over, whose tooltip is shown or pending.
let pending: HTMLElement | null = null;
let showTimer: number | undefined;
let installed = false;

function bubbleElement(): HTMLDivElement {
  if (bubble) {
    return bubble;
  }

  const element = document.createElement("div");
  element.id = "app-tooltip";
  element.setAttribute("role", "tooltip");
  // Manual: the platform never light-dismisses or toggles it — this module owns
  // its lifecycle, so it can safely coexist with an open menu popover.
  element.popover = "manual";
  document.body.append(element);
  bubble = element;
  return element;
}

// The nearest ancestor carrying a tooltip, or null when the event is elsewhere.
function tooltipTarget(event: Event): HTMLElement | null {
  const node = event.target;
  return node instanceof Element ? node.closest<HTMLElement>("[data-tooltip]") : null;
}

function show(target: HTMLElement): void {
  const text = target.getAttribute("data-tooltip");
  if (!text) {
    return;
  }

  const element = bubbleElement();
  element.textContent = text;
  element.dataset.pos = target.getAttribute("data-tooltip-pos") ?? "bottom";
  target.style.setProperty("anchor-name", ANCHOR_NAME);
  anchored = target;

  if (!element.matches(":popover-open")) {
    element.showPopover();
  }
}

function hide(): void {
  clearTimeout(showTimer);
  showTimer = undefined;

  if (anchored) {
    anchored.style.removeProperty("anchor-name");
    anchored = null;
  }

  if (bubble?.matches(":popover-open")) {
    bubble.hidePopover();
  }
}

// Single reconciler for both pointer and focus: whenever what's under the
// pointer / focused changes, retarget. Moving within the same control (e.g. onto
// a child) resolves to the same target and is a no-op, so there's no flicker.
function retarget(target: HTMLElement | null): void {
  if (target === pending) {
    return;
  }

  pending = target;
  hide();

  if (!target) {
    return;
  }

  showTimer = window.setTimeout(() => {
    if (pending === target && target.isConnected) {
      show(target);
    }
  }, SHOW_DELAY_MS);
}

function onPointerOver(event: PointerEvent): void {
  retarget(tooltipTarget(event));
}

function onFocusIn(event: FocusEvent): void {
  retarget(tooltipTarget(event));
}

// A blur, a leave to outside the window, a press, a scroll, or Escape all
// dismiss immediately — a tooltip must never outlive the intent that showed it.
function dismiss(): void {
  pending = null;
  hide();
}

function onKeyDown(event: KeyboardEvent): void {
  if (event.key === "Escape") {
    dismiss();
  }
}

/** Install the global tooltip controller. Idempotent — safe to call once. */
export function initTooltips(): void {
  if (installed) {
    return;
  }

  installed = true;
  document.addEventListener("pointerover", onPointerOver, true);
  document.addEventListener("focusin", onFocusIn, true);
  document.addEventListener("focusout", dismiss, true);
  document.addEventListener("pointerdown", dismiss, true);
  document.addEventListener("scroll", dismiss, true);
  document.addEventListener("keydown", onKeyDown, true);
  window.addEventListener("blur", dismiss);
  // Leaving the document entirely (pointer never re-enters a tooltip element).
  document.addEventListener("pointerleave", dismiss);
}
