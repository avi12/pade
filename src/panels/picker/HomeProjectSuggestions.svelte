<script lang="ts">
  import Icon from "@/lib/Icon.svelte";
  import { popoverOpenWhen } from "@/lib/ui/popover-open";
  import { isTemporaryWorkspace, projectRootPrefix } from "@/lib/util/paths";
  import RootedName from "@/lib/workspace/display/RootedName.svelte";
  import TempBadge from "@/lib/workspace/display/TempBadge.svelte";
  import type { ProjectOption } from "@/panels/picker/project-suggestions";

  // The existing projects a typed new-project name matches, under the name
  // field as a top-layer listbox anchored to it: one click (or arrows, then
  // Enter, handled by the field, whose focus never leaves) switches the
  // composer to that project instead of creating a new one. Each row reads
  // like the project menu's ("avi\pade").
  const { listboxId, anchorName, isOpen, suggestions, highlightedIndex, optionId, onpick }: {
    listboxId: string;
    /** The field's anchor-name, so the card sits under it at its width. */
    anchorName: string;
    isOpen: boolean;
    suggestions: readonly ProjectOption[];
    /** The row the field's arrows highlight, -1 while none. */
    highlightedIndex: number;
    optionId: (index: number) => string;
    onpick: (path: string) => void;
  } = $props();

  const headId = $derived(`${listboxId}-head`);
</script>

<div
  style:position-anchor={anchorName}
  class="typeahead"
  {@attach popoverOpenWhen(isOpen)}
  popover="manual"
>
  <div id={headId} class="menu-separator">Open existing</div>
  <ul id={listboxId} aria-labelledby={headId} role="listbox">
    {#each suggestions as project, index (project.path)}
      <li>
        <button
          id={optionId(index)}
          class="typeahead-option"
          class:active={index === highlightedIndex}
          aria-selected={index === highlightedIndex}
          onclick={() => onpick(project.path)}
          onmousedown={e => e.preventDefault()}
          role="option"
          type="button"
        >
          <Icon name="folder" size={15} />
          <span class="typeahead-text">
            <RootedName name={project.name} prefix={projectRootPrefix(project.path)} />
          </span>
          {#if isTemporaryWorkspace(project.path)}
            <TempBadge />
          {/if}
        </button>
      </li>
    {/each}
  </ul>
</div>

<style>
  ul {
    display: flex;
    flex-direction: column;
    gap: 2px;
    margin: 0;
    padding: 0;
    list-style: none;
  }
</style>
