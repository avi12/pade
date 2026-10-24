<script lang="ts">
  import { ProjectChoice } from "@/panels/picker/home-composer";
  import HomeMenuFooter from "@/panels/picker/HomeMenuFooter.svelte";
  import HomeProjectOption from "@/panels/picker/HomeProjectOption.svelte";

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
      options: readonly {
        path: string;
        name: string;
      }[];
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

<div id={menuId} style:position-anchor="--{menuId}" class="menu popover-menu home-menu" popover>
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
     it at 400px and 10px short of the viewport) and scrolls, but never
     below a 30px sliver; the foot keeps its full size. Only a foot taller
     than the whole cap leaves the menu itself to scroll. One width whatever
     tab the foot shows, wide enough for the Clone form. */
  .home-menu {
    --home-menu-min-inline-size: 360px;

    inline-size: max(anchor-size(inline), var(--home-menu-min-inline-size));
    max-inline-size: calc(100vw - 2 * var(--picker-menu-viewport-gap));

    &:popover-open {
      display: flex;
      flex-direction: column;
    }
  }

  .options {
    --home-menu-options-min-block-size: 30px;

    flex: 1 1 auto;
    overflow-y: auto;
    min-block-size: var(--home-menu-options-min-block-size);
    margin: 0;
    padding: 0;
    scrollbar-width: thin;
    list-style: none;
  }

  .none {
    padding: 8px 10px;
    color: var(--on-surface-variant);
    font-size: 0.8125rem;
  }
</style>
