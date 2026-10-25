<script lang="ts">
  import Icon from "@/lib/Icon.svelte";
  import { baseName, displaySeparator } from "@/lib/util/paths";
  import { NEW_PROJECT_PLACEHOLDER } from "@/panels/picker/home-composer";
  import RootSelect from "@/panels/picker/RootSelect.svelte";

  // The composer's project slot while a new project is picked: the trigger
  // turns into the name field itself, so the path reads as one line - the
  // root, then the name being typed. The root is its own small select, so
  // switching roots is one click on it; the trailing caret still opens the
  // project menu to pick a project or a throwaway instead.
  let {
    menuId,
    rootMenuId,
    fieldId,
    formId,
    prefix,
    roots,
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

  let elName = $state<HTMLInputElement | null>(null);
  const separator = $derived(displaySeparator(createIn || (roots[0] ?? "")));
  const accessibleName = $derived.by(() => {
    if (prefix) {
      return `${NAME_FIELD_LABEL} in ${prefix}`;
    }

    return NAME_FIELD_LABEL;
  });

  $effect(() => {
    const isFocusRequested = focusRequest > 0;
    if (isFocusRequested) {
      elName?.focus();
    }
  });
</script>

<div style:anchor-name="--{rootMenuId}" class="new-field field-trigger">
  <Icon name="folderPlus" size={15} />
  <span class="path">
    <RootSelect
      label={createIn ? baseName(createIn) : undefined}
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
      aria-describedby={isInvalid ? problemId : undefined}
      aria-invalid={isInvalid}
      aria-label={accessibleName}
      autocomplete="off"
      form={formId}
      placeholder={NEW_PROJECT_PLACEHOLDER}
      spellcheck="false"
      bind:value={name}
    />
  </span>
  <button class="caret-button" aria-label="Change project" popovertarget={menuId} type="button">
    <span aria-hidden="true">▾</span>
  </button>
</div>

<style>
  /* One control the size of the trigger it replaces: the field-trigger frame
     around a bare input, its border the single focus indicator. */
  .new-field {
    display: flex;
    flex: 1;
    gap: 8px;
    align-items: center;
    min-inline-size: 0;
    padding-inline-start: 14px;
    border-radius: var(--radius-medium);
    transition: border-color 150ms var(--ease);

    &:focus-within {
      border-color: var(--primary);
    }

    &:has([aria-invalid="true"]) {
      border-color: var(--critical);
    }
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

  /* The root reads as a select of its own - a tonal chip with its caret -
     so changing it is plainly one click away. */
  .path :global(.root-trigger) {
    padding-block: 3px;
    padding-inline: 8px 6px;
    border-radius: var(--radius-small);
    background: var(--surface-3);
    font-weight: 500;

    &:hover {
      background: var(--primary-container);
      color: var(--on-primary-container);
    }
  }

  /* Wide enough for its placeholder, so the hint never clips. */
  .name-input {
    --name-input-min-inline-size: 12ch;

    flex: 1 1 var(--name-input-min-inline-size);
    min-inline-size: var(--name-input-min-inline-size);
    padding-block: 10px;
    padding-inline: 0;
    border: none;
    background: transparent;
    color: var(--on-surface);
    outline: none;
    font-family: var(--font-monospace);
    font-weight: 400;
    font-size: 0.8125rem;
    box-shadow: none;
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

  /* The skin frames every control and draws an accent line under every
     field; here the whole control is the one plate, so the frame moves onto
     it and the input and caret carry none. */
  :global(:root[data-palette="cyberpunk"]) .new-field {
    outline: 1px solid var(--control-edge);
    outline-offset: -1px;

    /* The skin's focused-field ring, on the plate the input lives in. */
    &:focus-within {
      outline: 2px solid var(--hairline);
      outline-offset: -2px;
    }
  }

  :global(:root[data-palette="cyberpunk"]) .name-input {
    background: none;

    &:is(:focus-visible, :not(:focus-visible)) {
      outline: none;
    }
  }

  :global(:root[data-palette="cyberpunk"]) .caret-button:not(:focus-visible) {
    outline: none;
  }
</style>
