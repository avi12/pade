<script lang="ts">
  import Icon from "@/lib/Icon.svelte";
  import { baseName, displaySeparator } from "@/lib/util/paths";
  import { NEW_PROJECT_PLACEHOLDER } from "@/panels/picker/home-composer";
  import HomeProjectSuggestions from "@/panels/picker/HomeProjectSuggestions.svelte";
  import type { ProjectOption } from "@/panels/picker/project-suggestions";
  import { matchingProjects, suggestionKeyStep, SuggestionStepKind } from "@/panels/picker/project-suggestions";
  import RootSelect from "@/panels/picker/RootSelect.svelte";

  // The composer's project slot while a new project is picked: the trigger
  // turns into the name field itself, so the path reads as one line - the
  // root, then the name being typed. The root is its own small select, so
  // switching roots is one click on it. As a name is typed, existing projects
  // that match drop down under the field, so a name that is already a project
  // opens that one instead; the trailing caret opens the whole project menu.
  let {
    menuId,
    rootMenuId,
    fieldId,
    formId,
    prefix,
    roots,
    projects,
    onpick,
    onnewroot,
    problemId,
    isInvalid,
    focusRequest,
    createIn = $bindable(""),
    name = $bindable("")
  }: {
    /** The project menu's id - the caret opens it. */
    menuId: string;
    /** The fallback root select's menu id (and anchor). */
    rootMenuId: string;
    fieldId: string;
    /** The form Enter submits, so it starts like Start does. */
    formId: string;
    /** The root as it reads before the name ("avi\"), empty while none. */
    prefix: string;
    roots: string[];
    /** The project menu's projects, recents first - what the name suggests. */
    projects: readonly ProjectOption[];
    /** Switch the composer to an existing project, as a menu row does. */
    onpick: (path: string) => void;
    onnewroot: () => void;
    /** The id of the message saying why the name can't be created. */
    problemId: string;
    isInvalid: boolean;
    /** Bumped on every New pick: the name field takes focus each time, after
     *  the closing menu has handed focus back to its invoker. */
    focusRequest: number;
    createIn?: string;
    name?: string;
  } = $props();

  const NAME_FIELD_LABEL = "New project name";
  const CARET_LABEL = "Open a recent project";

  const listboxId = $derived(`${fieldId}-suggestions`);
  const anchorName = $derived(`--${rootMenuId}`);

  function optionId(index: number) {
    return `${listboxId}-${index}`;
  }

  // The suggestions show while the field is being typed in and something
  // matches; no row is highlighted until an arrow key picks one, so Enter
  // still starts the new project as Start does.
  let isListOpen = $state(false);
  let activeIndex = $state(-1);
  const suggestions = $derived(
    matchingProjects({
      options: projects,
      typed: name
    })
  );
  const isListVisible = $derived(isListOpen && suggestions.length > 0);
  const highlightedIndex = $derived.by(() => {
    const isHighlightShown = isListVisible && activeIndex < suggestions.length;
    if (!isHighlightShown) {
      return -1;
    }

    return activeIndex;
  });
  const activeOptionId = $derived.by(() => {
    if (highlightedIndex < 0) {
      return undefined;
    }

    return optionId(highlightedIndex);
  });

  let elName = $state<HTMLInputElement | null>(null);
  const separator = $derived(displaySeparator(createIn || (roots[0] ?? "")));
  const accessibleName = $derived.by(() => {
    if (prefix) {
      return `${NAME_FIELD_LABEL} in ${prefix}`;
    }

    return NAME_FIELD_LABEL;
  });
  // The chosen root's folder name on its chip; the select's own placeholder
  // while none is chosen.
  const rootLabel = $derived.by(() => {
    if (!createIn) {
      return undefined;
    }

    return baseName(createIn);
  });
  const describedBy = $derived.by(() => {
    if (!isInvalid) {
      return undefined;
    }

    return problemId;
  });

  $effect(() => {
    const isFocusRequested = focusRequest > 0;
    if (isFocusRequested) {
      elName?.focus();
    }
  });
</script>

<div style:anchor-name={anchorName} class="new-field text-field-plate menu-trigger">
  <Icon name="folderPlus" size={15} />
  <span class="path">
    <RootSelect
      isChip
      label={rootLabel}
      menuId={rootMenuId}
      {onnewroot}
      {roots}
      bind:value={createIn}
    />
    <span class="prefix separator" aria-hidden="true">{separator}</span>
    <input
      bind:this={elName}
      id={fieldId}
      class="name-input"
      aria-activedescendant={activeOptionId}
      aria-autocomplete="list"
      aria-controls={listboxId}
      aria-describedby={describedBy}
      aria-expanded={isListVisible}
      aria-invalid={isInvalid}
      aria-label={accessibleName}
      autocomplete="off"
      form={formId}
      onblur={() => (isListOpen = false)}
      oninput={() => {
        isListOpen = true;
        activeIndex = -1;
      }}
      onkeydown={e => {
        const step = suggestionKeyStep({
          key: e.key,
          highlightedIndex,
          count: suggestions.length,
          isListVisible
        });
        if (step === null) {
          return;
        }

        e.preventDefault();

        if (step.kind === SuggestionStepKind.Pick) {
          onpick(suggestions[step.index].path);
          return;
        }

        activeIndex = step.activeIndex;
        isListOpen = step.isListOpen;
      }}
      placeholder={NEW_PROJECT_PLACEHOLDER}
      role="combobox"
      spellcheck="false"
      bind:value={name}
    />
  </span>
  <button
    class="caret-button"
    aria-label={CARET_LABEL}
    data-tooltip={CARET_LABEL}
    popovertarget={menuId}
    type="button"
  >
    <span class="caret" aria-hidden="true">▾</span>
  </button>
  <HomeProjectSuggestions
    {anchorName}
    {highlightedIndex}
    isOpen={isListVisible}
    {listboxId}
    {onpick}
    {optionId}
    {suggestions}
  />
</div>

<style>
  /* One control the size of the trigger it replaces: the shared field plate
     around a bare input, the caret button running to its end edge. */
  .new-field {
    --field-plate-padding-inline-end: 0;

    display: flex;
    flex: 1;
    gap: 8px;
    align-items: center;
    min-inline-size: 0;
  }

  /* The root and the name share one line with no gap, so they read as one
     path. */
  .path {
    display: flex;
    flex: 1;
    align-items: center;
    min-inline-size: 0;
  }

  .prefix {
    flex: none;
    font-family: var(--font-monospace);
    font-weight: 400;
  }

  .separator {
    padding-inline: 2px;
    color: var(--on-surface-variant);
  }

  /* Wide enough for its placeholder, so the hint never clips. */
  .name-input {
    --name-input-min-inline-size: 12ch;

    flex: 1 1 var(--name-input-min-inline-size);
    min-inline-size: var(--name-input-min-inline-size);
    font-family: var(--font-monospace);
    font-weight: 400;
  }

  .caret-button {
    flex: none;
    align-self: stretch;
    padding-block: 0;
    padding-inline: 12px;
    border-radius: var(--radius-medium);
    background: transparent;
    color: inherit;
    font-size: 0.5625rem;
    opacity: 70%;
  }
</style>
