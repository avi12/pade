<script lang="ts">
  import { homeMenuPanelId, homeMenuTabId } from "@/panels/picker/home-menu-tabs";
  import type { HomeMenuTab } from "@/panels/picker/home-menu-tabs";
  import type { Snippet } from "svelte";

  // One panel of the composer menu's foot, and the slide between panels.
  // Every panel stays laid out (typed state survives switching, and the
  // tallest one sizes the foot); only the selected one is visible, and an
  // unselected one is inert at once, even while it is still sliding out.
  const { menuId, tab, isSelected, isForm, children }: {
    menuId: string;
    tab: HomeMenuTab;
    isSelected: boolean;
    /** A form sits a touch inside the menu's edge, where menu rows are
     *  full-bleed. */
    isForm: boolean;
    children: Snippet;
  } = $props();
</script>

<div
  id={homeMenuPanelId({
    menuId,
    tab
  })}
  class="tab-panel"
  class:form={isForm}
  class:selected={isSelected}
  aria-labelledby={homeMenuTabId({
    menuId,
    tab
  })}
  inert={!isSelected}
  role="tabpanel"
>
  {@render children()}
</div>

<style>
  /* The panels share one cell of HomeMenuFooter's `.panels` grid, so the
     foot is always as tall as its tallest panel and the list and tabs above
     never move on a switch; the selected panel is the anchor the menu's
     frame ends at (HomeProjectMenu), so only that edge moves. Each hangs
     from the cell's top at its own height.

     The selected one slides in from the side the selection moved toward
     (`--tab-slide-direction`, from the grid; `--tab-slide-duration`, from
     the menu) while the one it replaces
     slides out the other way and fades; the menu opening (direction 0) only
     fades it in. The global reduced-motion reset makes it an instant swap. */
  .tab-panel {
    --tab-slide-distance: 24px;

    grid-row: 1;
    grid-column: 1;
    align-self: start;

    &.form {
      padding-block: 4px 6px;
      padding-inline: 6px;
    }

    &.selected {
      pointer-events: auto;
      animation: tab-panel-in var(--tab-slide-duration) var(--ease);
      anchor-name: --home-menu-shown-panel;
    }

    &:not(.selected) {
      opacity: 0%;
      visibility: hidden;
      transition:
        opacity var(--tab-slide-duration) var(--ease),
        translate var(--tab-slide-duration) var(--ease),
        visibility var(--tab-slide-duration);
      translate: calc(-1 * var(--tab-slide-direction) * var(--tab-slide-distance)) 0;
    }
  }

  @keyframes tab-panel-in {
    from {
      opacity: 0%;
      translate: calc(var(--tab-slide-direction) * var(--tab-slide-distance)) 0;
    }
  }
</style>
