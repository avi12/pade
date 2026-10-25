<script lang="ts">
  import { homeMenuPanelId, homeMenuTabId } from "@/panels/picker/home-menu-tabs";
  import type { HomeMenuTab } from "@/panels/picker/home-menu-tabs";
  import type { Snippet } from "svelte";

  // One panel of the composer menu's foot, and the slide between panels.
  // Every panel stays mounted (typed state survives switching); only the
  // selected one is displayed, and an unselected one is inert at once, even
  // while it is still sliding out.
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
  /* The panels live in HomeMenuFooter's `.panels` grid, which sets
     `--tab-slide-direction`. The selected one slides in from the side the
     selection moved toward while the one it replaces slides out the other
     way and fades, its `display: none` held back until the fade ends
     (`allow-discrete`). The global reduced-motion reset turns both into an
     instant swap.

     The height follows in the same stroke: the selected panel sits in the
     first row and grows from nothing to its own height while the one it
     replaces, in the second row, shrinks to nothing - one easing for both,
     so the two rows always add up to a smooth blend of the old height into
     the new. The leaving panel's content hangs from its bottom edge, so it
     still paints over the arriving one as it slides away instead of being
     pushed down below it. */
  .tab-panel {
    --tab-slide-distance: 24px;

    grid-row: 1;
    grid-column: 1;
    min-block-size: 0;
    transition:
      opacity 200ms var(--ease),
      translate 200ms var(--ease),
      block-size 200ms var(--ease),
      padding-block 200ms var(--ease),
      display 200ms allow-discrete;
    translate: 0;
    interpolate-size: allow-keywords;

    &.form {
      padding-block: 4px 6px;
      padding-inline: 6px;
    }

    &:not(.selected) {
      display: none;
      grid-row: 2;
      align-content: unsafe end;
      block-size: 0;
      padding-block: 0;
      opacity: 0%;
      translate: calc(-1 * var(--tab-slide-direction) * var(--tab-slide-distance)) 0;
    }
  }

  @starting-style {
    .tab-panel.selected {
      opacity: 0%;
      translate: calc(var(--tab-slide-direction) * var(--tab-slide-distance)) 0;
    }
  }

  /* Only a tab switch grows the arriving panel from nothing: the menu
     opening (direction 0) shows it at its full height at once. */
  @container not style(--tab-slide-direction: 0) {
    @starting-style {
      .tab-panel.selected {
        block-size: 0;
        padding-block: 0;
      }
    }
  }
</style>
