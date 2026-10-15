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
    createIn = $bindable(""),
    name = $bindable("")
  }: {
    roots: string[];
    onnewroot: () => void;
    nameError: string | null;
    isTakenByFile: boolean;
    isExistingFolder: boolean;
    createIn?: string;
    name?: string;
  } = $props();

  const NAME_ERROR_ID = "new-project-name-error";
  const describedBy = $derived(nameError ? NAME_ERROR_ID : undefined);
</script>

<div class="new-project-field">
  <span id="new-project-location-label" class="new-project-label">Location</span>
  <div
    style:anchor-name="--new-project-root-menu-create"
    class="new-project-location"
    aria-labelledby="new-project-location-label"
    role="group"
  >
    <span class="new-project-location-icon" aria-hidden="true"><Icon name="folder" /></span>
    <RootSelect menuId="new-project-root-menu-create" {onnewroot} {roots} bind:value={createIn} />
    <span class="new-project-separator" aria-hidden="true">\</span>
    <label class="visually-hidden" for="new-project-name">Project name</label>
    <input
      id="new-project-name"
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
      id={NAME_ERROR_ID}
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
