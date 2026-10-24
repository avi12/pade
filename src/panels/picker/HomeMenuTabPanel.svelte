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
  /* The panels share one grid cell (HomeMenuFooter's `.panels`, which sets
     `--tab-slide-direction`). The selected one slides in from the side the
     selection moved toward while the one it replaces slides out the other
     way and fades, its `display: none` held back until the fade ends
     (`allow-discrete`). The global reduced-motion reset turns both into an
     instant swap. */
  .tab-panel {
    --tab-slide-distance: 24px;

    grid-area: 1 / 1;
    transition:
      opacity 200ms var(--ease),
      translate 200ms var(--ease),
      display 200ms allow-discrete;
    translate: 0;

    &:not(.selected) {
      display: none;
      opacity: 0%;
      translate: calc(-1 * var(--tab-slide-direction) * var(--tab-slide-distance)) 0;
    }

    &.form {
      padding-block: 4px 6px;
      padding-inline: 6px;
    }
  }

  @starting-style {
    .tab-panel.selected {
      opacity: 0%;
      translate: calc(var(--tab-slide-direction) * var(--tab-slide-distance)) 0;
    }
  }
</style>
