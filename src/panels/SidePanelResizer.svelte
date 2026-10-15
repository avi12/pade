<script lang="ts">
  import { KeyName } from "@/lib/key-names";
  import {
    effective,
    SIDE_PANEL_DEFAULT_WIDTH,
    SIDE_PANEL_MAX_FRACTION,
    SIDE_PANEL_MIN_WIDTH,
    updatePrefs
  } from "@/lib/prefs.svelte";

  // Draggable divider between the terminal area and the side panel. The
  // persisted width lives in prefs (SSOT); `width` is the local mirror the drag
  // moves live without a backend write per frame, bound up to the `.body` grid
  // that lays the column out (capped at 60% of the window by a CSS clamp, so the
  // terminal is never swallowed). Mirrors the design's `panelW` +
  // `startPanelResize`/`resetPanelW`.
  let { width = $bindable(), isResizing = $bindable() }: {
    width: number;
    isResizing: boolean;
  } = $props();

  const NUDGE_PIXELS = 16;
  const BODY_SELECTOR = ".body";
  const PRIMARY_BUTTON = 0;

  // Adopt a width persisted in prefs (e.g. loaded at boot, or reset) - but never
  // while a drag is live, so the pointer stays in charge of the frame.
  $effect(() => {
    const persisted = effective.sidePanelWidth;
    if (!isResizing) {
      width = persisted;
    }
  });

  // `updatePrefs` assigns `prefs` synchronously before its first await, so
  // calling this before clearing `isResizing` means the adopt-effect already sees
  // the new value and won't briefly revert the drag. A failed settings write just
  // keeps the width on screen; there is nothing to undo.
  async function persistWidth() {
    await Promise.allSettled([updatePrefs({ sidePanelWidth: Math.round(width) })]);
  }

  // Clamp a proposed width to [min, 60% of the body] - the same ceiling the CSS
  // clamp enforces, so keyboard and pointer agree.
  function clampWidth(proposed: number) {
    const elBody = document.querySelector<HTMLElement>(BODY_SELECTOR);
    const ceiling = elBody ? Math.round(elBody.getBoundingClientRect().width * SIDE_PANEL_MAX_FRACTION) : proposed;
    return Math.max(SIDE_PANEL_MIN_WIDTH, Math.min(ceiling, Math.round(proposed)));
  }

  async function nudge(delta: number) {
    width = clampWidth(width + delta);
    await persistWidth();
  }

  async function reset() {
    width = SIDE_PANEL_DEFAULT_WIDTH;
    await persistWidth();
  }
</script>

<!-- A thin bar (var(--outline)) that grows to full-height --primary on hover /
     drag / focus. Sits on the grid boundary via the same clamp the column uses;
     double-click resets, arrows nudge. Mirrors the design's
     [data-resizehandle]/[data-resizebar]. -->
<!-- A focusable, keyboard-operable window-splitter is a valid ARIA separator
     widget (tabindex + aria-valuenow), so the non-interactive separator
     warnings are false positives here. -->
<!-- svelte-ignore a11y_no_noninteractive_tabindex -->
<!-- svelte-ignore a11y_no_noninteractive_element_interactions -->
<div
  style:--side-panel-width="{width}px"
  class="panel-resize"
  aria-label="Resize side panel (double-click to reset)"
  aria-orientation="vertical"
  aria-valuemin={SIDE_PANEL_MIN_WIDTH}
  aria-valuenow={Math.round(width)}
  data-tooltip="Drag to resize · double-click to reset"
  ondblclick={reset}
  onkeydown={async e => {
    if (e.key === KeyName.ArrowLeft) {
      e.preventDefault();
      await nudge(NUDGE_PIXELS);
    } else if (e.key === KeyName.ArrowRight) {
      e.preventDefault();
      await nudge(-NUDGE_PIXELS);
    } else if (e.key === KeyName.Home) {
      e.preventDefault();
      await reset();
    }
  }}
  onpointerdown={e => {
    if (e.button !== PRIMARY_BUTTON || !(e.currentTarget instanceof HTMLElement)) {
      return;
    }

    e.preventDefault();
    const elHandle = e.currentTarget;
    elHandle.setPointerCapture(e.pointerId);
    isResizing = true;
    document.body.style.cursor = "col-resize";
    document.body.style.userSelect = "none";

    function onMove(move: PointerEvent) {
      const elBody = elHandle.closest<HTMLElement>(BODY_SELECTOR);
      if (!elBody) {
        return;
      }

      width = clampWidth(elBody.getBoundingClientRect().right - move.clientX);
    }
    async function finish() {
      elHandle.removeEventListener("pointermove", onMove);
      elHandle.removeEventListener("pointerup", finish);
      elHandle.removeEventListener("pointercancel", finish);
      document.body.style.cursor = "";
      document.body.style.userSelect = "";
      // Persist first (updates prefs synchronously), then release the drag
      // guard so the adopt-effect sees the final width, not the old.
      const persisting = persistWidth();
      isResizing = false;
      await persisting;
    }
    elHandle.addEventListener("pointermove", onMove);
    elHandle.addEventListener("pointerup", finish);
    elHandle.addEventListener("pointercancel", finish);
  }}
  role="separator"
  tabindex="0"
><span class="panel-resize-bar" aria-hidden="true"></span></div>

<style>
  /* Straddles the terminal/side-panel boundary. Positioned on the grid line via
     the same clamp the column uses, and pulled half its width left so the
     hit-area sits over the seam. Lives in `.body` (not the clipped `.side-pane`)
     so it can overhang the boundary. */
  .panel-resize {
    position: absolute;
    inset-block: 0;
    inset-inline-end: var(--side-panel-track);
    z-index: 20;
    display: flex;
    justify-content: center;
    align-items: center;
    inline-size: 11px;
    margin-inline-end: -5.5px;
    cursor: col-resize;
    touch-action: none;

    /* Follows the seam in step with the column tween on a double-click reset,
       held still during a live drag (see `[data-resizing]` below). */
    transition: inset-inline-end 250ms var(--ease);

    @media (prefers-reduced-motion: reduce) {
      transition: none;
    }

    /* Stacked top/bottom below this width - a vertical col-resize divider no
       longer maps to the layout, so hide it. */
    @media (width <= 720px) {
      display: none;
    }

    :global(.body[data-resizing]) & {
      transition: none;
    }

    /* A thin pill at rest; fills to full-height primary while hovered, dragged,
       or keyboard-focused (design's [data-resizebar] hover/active rule). */
    .panel-resize-bar {
      block-size: 36px;
      inline-size: 2px;
      border-radius: var(--radius-full);
      background: var(--outline);
      transition: background 150ms var(--ease), block-size 150ms var(--ease);

      @media (prefers-reduced-motion: reduce) {
        transition: none;
      }
    }

    &:hover .panel-resize-bar,
    &:active .panel-resize-bar,
    &:focus-visible .panel-resize-bar {
      block-size: 100%;
      background: var(--primary);
    }

    &:focus-visible {
      outline: none;
    }
  }
</style>
