<script lang="ts">
  import { ProjectChoice } from "@/panels/picker/home-composer";
  import HomeNewSection from "@/panels/picker/HomeNewSection.svelte";
  import HomeProjectOption from "@/panels/picker/HomeProjectOption.svelte";

  // The composer's project menu: recent and scanned projects, each with its
  // root ("avi/pade"), then the New section pinned to the list's foot.
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

<ul id={menuId} style:position-anchor="--{menuId}" class="menu popover-menu" popover>
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
  <HomeNewSection
    isSeparated={hasOptions}
    isTempPicked={choice === ProjectChoice.Temp}
    {menuId}
    {nameFieldId}
    {onnew}
    {ontemp}
    {roots}
  />
</ul>

<style>
  .none {
    padding: 8px 10px;
    color: var(--on-surface-variant);
    font-size: 0.8125rem;
  }
</style>
