<script lang="ts">
  import { workspace } from "@/lib/bridge";
  import Icon from "@/lib/Icon.svelte";
  import { collapseRow, expandRow, flipDuration } from "@/lib/motion";
  import { normalizePath } from "@/lib/paths";
  import { emptyPathProbe } from "@/lib/types";
  import type { TaggedPathProbe } from "@/lib/types";
  import { flip } from "svelte/animate";

  // A folder-path text field with directory autocomplete. As the path is typed
  // the backend is probed (debounced) for what it is on disk plus its child
  // folders, which drop down as a top-layer popover listbox the user can arrow
  // through, Enter to accept, or Tab to accept-and-drill (append a separator and
  // keep completing sub-folders). This component owns only the typeahead; the
  // probe's full result flows back through `bind:probe` so the host builds its
  // own status/validation from it. Shared by the add-root field and the "open a
  // local folder" field so the completion behaviour has a single home.
  let {
    value = $bindable(""),
    probe = $bindable({
      path: "",
      result: emptyPathProbe
    }),
    name,
    id,
    inputClass = "",
    placeholder = "",
    inputElement = $bindable(null)
  }: {
    value?: string;
    probe?: TaggedPathProbe;
    // A unique key per instance — derives the listbox id, option ids, and the
    // anchor-name so two combos on one page never collide.
    name: string;
    // The input's own id, for an external `<label for=…>` to pair with.
    id?: string;
    inputClass?: string;
    placeholder?: string;
    inputElement?: HTMLInputElement | null;
  } = $props();

  const listboxId = $derived(`${name}-suggestions`);
  const anchorName = $derived(`--${name}-combo`);

  function optionId(index: number) {
    return `${name}-suggestion-${index}`;
  }

  let listOpen = $state(false);
  // The selected suggestion. One option is always selected while the list is
  // visible — the reconciler effect below enforces it; -1 only while hidden.
  let activeIndex = $state(-1);
  // What was last selected, remembered across re-filters so the reconciler can
  // follow the same folder to its new position (or fall to the nearest one).
  let lastSelected: {
    dir: string;
    index: number;
  } | null = null;
  let listEl = $state<HTMLUListElement | null>(null);

  // The OS path separator — appended when a highlighted suggestion is Tab-accepted
  // so the user can keep drilling into its sub-folders. The webview has no
  // `path.sep`, so read the platform the way OnLaunchSection already does.
  const pathSeparator = navigator.userAgent.includes("Windows") ? "\\" : "/";

  const trimmed = $derived(value.trim());
  // Never echo the exact folder already typed back as a suggestion (case- and
  // trailing-separator-insensitive, so a `…\` variant is still recognised as self).
  const suggestions = $derived(
    probe.result.suggestions.filter(dir => normalizePath(dir) !== normalizePath(probe.path))
  );
  const listVisible = $derived(listOpen && suggestions.length > 0);
  const activeId = $derived(activeIndex >= 0 ? optionId(activeIndex) : undefined);

  // Debounced backend probe: what the typed path is on disk (and whether its
  // parent exists) + directory completions. The result flows to the host via the
  // bound `probe`. Empty field clears it immediately.
  $effect(() => {
    const path = trimmed;
    if (path.length === 0) {
      probe = {
        path: "",
        result: emptyPathProbe
      };
      // A fresh path starts a fresh selection — don't carry one across a clear.
      lastSelected = null;
      return;
    }

    const timer = setTimeout(async () => {
      const result = await workspace.probePath(path);
      probe = {
        path,
        result
      };
    }, 120);
    return () => clearTimeout(timer);
  });

  // Drive the manual popover's top-layer visibility off `listVisible`. A typeahead
  // opens on input/focus rather than a popovertarget click, so it's shown and
  // hidden imperatively; the `:popover-open` guard keeps show/hidePopover idempotent.
  $effect(() => {
    const list = listEl;
    if (!list) {
      return;
    }

    const isOpen = list.matches(":popover-open");
    if (listVisible && !isOpen) {
      list.showPopover();
    } else if (!listVisible && isOpen) {
      list.hidePopover();
    }
  });

  // Move the selection — shared by the arrow keys and the reconciler, so the
  // remembered selection always matches what's highlighted.
  function select(index: number) {
    activeIndex = index;
    lastSelected = {
      dir: suggestions[index],
      index
    };
  }

  // While the list shows, one option is always selected, by one formula:
  //
  //   next = survivorIndex ≥ 0 ? survivorIndex : min(previousIndex, lastIndex)
  //
  // — a surviving option is followed by value to its new position; a vanished
  // one falls to the nearest remaining position (its old index, clamped to the
  // new end — a removed bottom selects the new bottom); no previous selection
  // means previousIndex 0, the top.
  $effect(() => {
    if (!listVisible) {
      return;
    }

    const survivorIndex = lastSelected ? suggestions.indexOf(lastSelected.dir) : -1;
    const previousIndex = lastSelected?.index ?? 0;
    const lastIndex = suggestions.length - 1;
    select(survivorIndex >= 0 ? survivorIndex : Math.min(previousIndex, lastIndex));
  });

  /** Focus the field — how a host lands the user here (the quick-start form's
   *  "New root folder…" jump, or a folder dropped on the local field). */
  export function focus() {
    inputElement?.focus();
  }

  // Adopt a suggested directory — shared by the listbox rows and the Enter key,
  // so it stays a named function.
  function pick(dir: string) {
    value = dir;
    listOpen = false;
    activeIndex = -1;
    lastSelected = null;
    inputElement?.focus();
  }
</script>

<input
  bind:this={inputElement}
  {id}
  style:anchor-name={anchorName}
  class="path-input {inputClass}"
  aria-activedescendant={activeId}
  aria-autocomplete="list"
  aria-controls={listboxId}
  aria-expanded={listVisible}
  autocomplete="off"
  onblur={() => (listOpen = false)}
  onfocus={() => (listOpen = true)}
  oninput={() => (listOpen = true)}
  onkeydown={e => {
    // Ctrl+Space re-opens the completions (IDE-style) — after an Escape, or to
    // summon the current folder's children without retyping. Suppress the space
    // it would otherwise insert.
    const isOpenListChord = e.ctrlKey && e.key === " ";
    if (isOpenListChord) {
      e.preventDefault();
      listOpen = true;
      return;
    }

    if (!listVisible) {
      return;
    }

    if (e.key === "ArrowDown") {
      e.preventDefault();
      select((activeIndex + 1) % suggestions.length);
    } else if (e.key === "ArrowUp") {
      e.preventDefault();
      select(activeIndex <= 0 ? suggestions.length - 1 : activeIndex - 1);
    } else if (e.key === "Enter" && activeIndex >= 0) {
      e.preventDefault();
      pick(suggestions[activeIndex]);
    } else if (e.key === "Tab" && activeIndex >= 0) {
      // Accept the highlighted folder and append a separator instead of moving
      // focus, then keep the list open so the debounced probe immediately starts
      // completing that folder's sub-folders — where the selection starts fresh
      // at the top.
      e.preventDefault();
      value = suggestions[activeIndex] + pathSeparator;
      lastSelected = null;
      listOpen = true;
    } else if (e.key === "Escape") {
      listOpen = false;
      activeIndex = -1;
      lastSelected = null;
    }
  }}
  {placeholder}
  role="combobox"
  spellcheck="false"
  type="text"
  bind:value
/>
<!-- Autocomplete listbox as a top-layer popover, anchored to the field with CSS
     anchor positioning: the top layer escapes the picker's own scroll clip, and
     flip-block flips it above the field near the viewport bottom, so it can never
     clip through the edge. Shown/hidden via showPopover/hidePopover from the
     `listVisible` effect (a typeahead opens on typing, not a popovertarget click). -->
<ul
  bind:this={listEl}
  id={listboxId}
  style:position-anchor={anchorName}
  class="suggestions"
  popover="manual"
  role="listbox"
>
  {#each suggestions as dir, index (dir)}
    <li in:expandRow out:collapseRow animate:flip={{ duration: flipDuration() }}>
      <button
        id={optionId(index)}
        class="suggestion"
        class:active={index === activeIndex}
        aria-selected={index === activeIndex}
        onclick={() => pick(dir)}
        onmousedown={e => e.preventDefault()}
        role="option"
        type="button"
      >
        <Icon name="folder" size={15} />
        <span class="sug-path">{dir}</span>
      </button>
    </li>
  {/each}
</ul>

<style>
  /* Directory autocomplete — a card of child folders shown as a top-layer popover
     anchored to the field. The top layer lifts it out of the picker's own scroll
     container (so it never clips), `anchor-size(width)` matches the field's width,
     and `flip-block` flips it above the field near the viewport bottom. The reveal
     transition and `margin: 0` come from the shared `[popover]` base. The
     `position-anchor` is set inline (per instance) so two combos never collide. */
  .suggestions {
    position: absolute;
    inset: auto;
    display: flex;
    flex-direction: column;
    gap: 2px;
    overflow-y: auto;
    max-block-size: 280px;
    inline-size: anchor-size(width);
    margin-block: 6px 0;
    padding: 6px;
    border: 1px solid var(--outline);
    border-radius: var(--radius-medium);
    background: var(--surface-2);
    list-style: none;
    box-shadow: 0 16px 40px var(--shadow-color);
    position-area: bottom span-right;
    position-try-fallbacks: flip-block;
  }

  .suggestion {
    display: flex;
    gap: 10px;
    align-items: center;
    inline-size: 100%;
    padding: 8px 10px;
    border: none;
    border-radius: var(--radius-small);
    background: transparent;
    color: var(--on-surface);
    font-family: var(--font-monospace);
    font-weight: 500;
    font-size: 12px;
    text-align: start;

    &:hover,
    &.active {
      background: var(--primary-container);
      color: var(--on-primary-container);
      filter: none;
    }

    .sug-path {
      overflow: hidden;
      text-overflow: ellipsis;
      white-space: nowrap;
    }
  }
</style>
