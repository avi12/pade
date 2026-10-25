<script lang="ts">
  import { arrowLists } from "@/lib/ui/keyboard/roving-menu";
  import { ProjectChoice } from "@/panels/picker/home-composer";
  import HomeMenuFooter from "@/panels/picker/HomeMenuFooter.svelte";
  import HomeProjectOption from "@/panels/picker/HomeProjectOption.svelte";
  import type { ProjectOption } from "@/panels/picker/project-suggestions";

  // The composer's project menu: recent and scanned projects, each with its
  // root ("avi/pade"), in a list that scrolls, then the New / Open / Clone
  // foot, which never scrolls away.
  let {
    menuId,
    groups,
    roots,
    choice,
    selectedPath,
    onpick,
    onnew,
    ontemp,
    onnewroot,
    createIn = $bindable("")
  }: {
    menuId: string;
    groups: readonly {
      head: string;
      isSeparated: boolean;
      options: readonly ProjectOption[];
    }[];
    roots: string[];
    choice: ProjectChoice;
    selectedPath: string;
    /** A project row, or a folder opened or cloned in the foot. */
    onpick: (path: string) => void;
    onnew: (root: string) => void;
    ontemp: () => void;
    onnewroot: () => void;
    createIn?: string;
  } = $props();

  const hasOptions = $derived(groups.length > 0);
  const pickedPath = $derived(choice === ProjectChoice.Existing ? selectedPath : null);
</script>

<!-- Keyboard: the project rows are one Tab stop walked by Up/Down, then the
     selected tab (Left/Right switch tabs), then the open panel's first field;
     Down from the New tab steps into its items. Opening focuses the first
     row, or the selected tab when there are none; Escape closes and hands
     focus back to the trigger (the popover's own behaviour). -->
<div
  id={menuId}
  style:position-anchor="--{menuId}"
  class="menu popover-menu home-menu"
  {@attach arrowLists}
  popover
>
  <ul class="options" aria-label="Projects">
    {#each groups as group (group.head)}
      <li class="head" class:separator={group.isSeparated} aria-hidden="true">{group.head}</li>
      {#each group.options as option (option.path)}
        <HomeProjectOption
          name={option.name}
          isPicked={option.path === pickedPath}
          {menuId}
          onpick={() => onpick(option.path)}
          path={option.path}
        />
      {/each}
    {/each}
    {#if !hasOptions}
      <li class="none">No projects yet - start one below</li>
    {/if}
  </ul>
  <HomeMenuFooter
    isSeparated={hasOptions}
    isTempPicked={choice === ProjectChoice.Temp}
    {menuId}
    {onnew}
    {onnewroot}
    {onpick}
    {ontemp}
    {roots}
    bind:createIn
  />
</div>

<style>
  /* A column: the project list takes the room the menu has (chrome.css caps
     it 10px short of the viewport), up to its own 320px, and scrolls, but
     never below three rows; the foot keeps its full size, which is always
     its tallest panel's (HomeMenuTabPanel). So the list and the tabs never
     move on a switch, and even the tallest panel ends short of the viewport.
     Only a foot taller than the whole room leaves the menu itself to scroll.
     One width whatever tab the foot shows, wide enough for the Clone form.
     The column holds while the menu fades out too, not only while open. */
  .home-menu {
    --home-menu-min-inline-size: 360px;

    /* One switch, one clock: the panels slide (HomeMenuTabPanel) while the
       frame's edge glides to the new one's end. */
    --tab-slide-duration: 200ms;

    flex-direction: column;
    inline-size: max(anchor-size(inline), var(--home-menu-min-inline-size));
    max-inline-size: calc(100vw - 2 * var(--picker-menu-viewport-gap));

    /* The frame is a layer of its own, ending at the shown panel: the box
       itself runs on to the tallest panel's end, unseen, and lets clicks in
       that stretch through to the page so they still dismiss the menu. */

    /* The box's own border gives way to the same room in padding, and the
       frame layer covers the whole box. */
    padding: calc(var(--popover-menu-padding) + var(--popover-menu-border-width));
    border: none;
    background: none;
    pointer-events: none;
    anchor-scope: --home-menu-shown-panel;

    &:popover-open {
      display: flex;
    }

    &::before {
      content: "";
      position: absolute;
      inset: 0;
      inset-block-end:
        calc(
          anchor(--home-menu-shown-panel bottom) - var(--popover-menu-padding)
          - var(--popover-menu-border-width)
        );
      z-index: -1;
      border: var(--popover-menu-border-width) solid var(--popover-menu-border-color);
      border-radius: inherit;
      background: var(--popover-menu-background);
      pointer-events: auto;
      transition: inset-block-end var(--tab-slide-duration) var(--ease);
    }
  }

  /* The scan-length cap is the list's own, where chrome.css caps a whole
     picker menu: here the foot below the list would eat into it. It always
     opens downward (opened upward, its frame would end short of the
     trigger), and never scrolls itself: a window too short for the list's
     three rows and the tallest panel lets the menu run past its edge. */
  .home-menu.menu {
    --picker-menu-max-block-size: 100%;

    overflow: visible;
    position-try-fallbacks: none;
  }

  .menu {
    /* Uppercase group header over the project list. */
    .head {
      padding-block: 6px 3px;
      padding-inline: 10px;
      color: var(--on-surface-variant);
      font-weight: 700;
      font-size: 0.625rem;
      letter-spacing: 0.07em;
      text-transform: uppercase;
    }

    .separator {
      margin-block-start: 6px;
      padding-block-start: 9px;
      border-block-start: 1px solid var(--outline);
    }
  }

  .options {
    /* The group head and three whole project rows. */
    --home-menu-options-min-block-size: 121px;
    --home-menu-options-max-block-size: 320px;

    flex: 1 1 auto;
    overflow-y: auto;
    min-block-size: var(--home-menu-options-min-block-size);
    max-block-size: var(--home-menu-options-max-block-size);
    margin: 0;
    padding: 0;
    scrollbar-width: thin;
    list-style: none;
    pointer-events: auto;
  }

  .none {
    padding: 8px 10px;
    color: var(--on-surface-variant);
    font-size: 0.8125rem;
  }
</style>
