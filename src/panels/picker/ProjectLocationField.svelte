<script lang="ts">
  import Icon from "@/lib/Icon.svelte";
  import { collapseRow, expandRow } from "@/lib/ui/motion";
  import RootSelect from "@/panels/picker/RootSelect.svelte";

  // The New tab's Location field: root select + project name in one group row,
  // with the inline reason the name can't be created - the schema's message, or
  // a file or folder already on disk under that name.
  let {
    roots,
    onnewroot,
    nameError,
    isTakenByFile,
    isExistingFolder,
    menuId = "new-project-root-menu-create",
    fieldId = "new-project-name",
    createIn = $bindable(""),
    name = $bindable("")
  }: {
    roots: string[];
    onnewroot: () => void;
    nameError: string | null;
    isTakenByFile: boolean;
    isExistingFolder: boolean;
    /** The root menu's id (and anchor) - passed when two location rows share
     *  the page, so their popovers never answer for each other. */
    menuId?: string;
    /** The name input's id - passed for the same reason as `menuId`. */
    fieldId?: string;
    createIn?: string;
    name?: string;
  } = $props();

  const describedBy = $derived(nameError ? `${fieldId}-error` : undefined);
</script>

<div class="new-project-field">
  <span id="{fieldId}-location-label" class="new-project-label">Location</span>
  <div
    style:anchor-name="--{menuId}"
    class="new-project-location"
    aria-labelledby="{fieldId}-location-label"
    role="group"
  >
    <span class="new-project-location-icon" aria-hidden="true"><Icon name="folder" /></span>
    <RootSelect {menuId} {onnewroot} {roots} bind:value={createIn} />
    <span class="new-project-separator" aria-hidden="true">\</span>
    <label class="visually-hidden" for={fieldId}>Project name</label>
    <input
      id={fieldId}
      class="new-project-name"
      aria-describedby={describedBy}
      aria-invalid={nameError !== null}
      autocomplete="off"
      placeholder="project-name"
      spellcheck="false"
      bind:value={name}
    />
  </div>
  {#if nameError}
    <output
      id="{fieldId}-error"
      class="field-error"
      in:expandRow
      out:collapseRow
    >{nameError}</output>
  {:else if isTakenByFile}
    <output class="field-error" in:expandRow out:collapseRow>
      That name is taken by a file in this root
    </output>
  {:else if isExistingFolder}
    <output class="field-error" in:expandRow out:collapseRow>
      This folder already exists in this root
    </output>
  {/if}
</div>
