<script lang="ts">
  import Icon from "@/lib/Icon.svelte";
  import { collapseRow, expandRow } from "@/lib/ui/motion";
  import { NEW_PROJECT_PLACEHOLDER, newProjectNameProblem } from "@/panels/picker/home-composer";
  import RootSelect from "@/panels/picker/RootSelect.svelte";

  // The composer's new-project name field, with the inline reason the name
  // can't be created - the schema's message, or a file or folder already on
  // disk under that name. The root was picked in the project menu (the
  // trigger shows it), so the root select appears only while none is chosen.
  let {
    roots,
    onnewroot,
    nameError,
    isTakenByFile,
    isExistingFolder,
    menuId,
    fieldId,
    focusRequest,
    createIn = $bindable(""),
    name = $bindable("")
  }: {
    roots: string[];
    onnewroot: () => void;
    nameError: string | null;
    isTakenByFile: boolean;
    isExistingFolder: boolean;
    /** The root menu's id (and anchor). */
    menuId: string;
    /** The name input's id. */
    fieldId: string;
    /** Bumped on every New pick: the name field takes focus each time, after
     *  the closing menu has handed focus back to its trigger. */
    focusRequest: number;
    createIn?: string;
    name?: string;
  } = $props();

  let elName = $state<HTMLInputElement | null>(null);
  const problem = $derived(
    newProjectNameProblem({
      nameError,
      isTakenByFile,
      isExistingFolder
    })
  );
  const errorId = $derived(`${fieldId}-error`);

  $effect(() => {
    const isFocusRequested = focusRequest > 0;
    if (isFocusRequested) {
      elName?.focus();
    }
  });
</script>

<div class="new-project-field">
  <label class="new-project-label" for={fieldId}>Project name</label>
  <div style:anchor-name="--{menuId}" class="new-project-location">
    <span class="new-project-location-icon" aria-hidden="true"><Icon name="folderPlus" /></span>
    {#if !createIn}
      <RootSelect {menuId} {onnewroot} {roots} bind:value={createIn} />
      <span class="new-project-separator" aria-hidden="true">\</span>
    {/if}
    <input
      bind:this={elName}
      id={fieldId}
      class="new-project-name"
      aria-describedby={problem === null ? undefined : errorId}
      aria-invalid={problem !== null}
      autocomplete="off"
      placeholder={NEW_PROJECT_PLACEHOLDER}
      spellcheck="false"
      bind:value={name}
    />
  </div>
  {#if problem !== null}
    <output id={errorId} class="field-error" in:expandRow out:collapseRow>{problem}</output>
  {/if}
</div>
