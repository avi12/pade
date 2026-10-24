<script lang="ts">
  import { ProjectChoice } from "@/panels/picker/home-composer";
  import HomeNewSection from "@/panels/picker/HomeNewSection.svelte";
  import HomeProjectOption from "@/panels/picker/HomeProjectOption.svelte";

  // The composer's project menu: recent and scanned projects, each with its
  // root ("avi/pade"), in a list that scrolls, then the New section at the
  // foot, which never scrolls away.
  const { menuId, groups, roots, choice, selectedPath, nameFieldId, onpick, onnew, ontemp }: {
    menuId: string;
    groups: readonly {
      head: string;
      isSeparated: boolean;
      options: readonly {
        path: string;
        name: string;
      }[];
    }[];
    roots: readonly string[];
    choice: ProjectChoice;
    selectedPath: string;
    nameFieldId: string;
    onpick: (path: string) => void;
    onnew: (root: string) => void;
    ontemp: () => void;
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
  <HomeNewSection
    isSeparated={hasOptions}
    isTempPicked={choice === ProjectChoice.Temp}
    {menuId}
    {nameFieldId}
    {onnew}
    {ontemp}
    {roots}
  />
</div>

<style>
  /* A column: the project list takes the room the menu has (chrome.css caps
     it at 400px and 10px short of the viewport) and scrolls, but never
     below a 30px sliver; the foot keeps its full size. Only a foot taller
     than the whole cap leaves the menu itself to scroll. */
  .home-menu:popover-open {
    display: flex;
    flex-direction: column;
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
