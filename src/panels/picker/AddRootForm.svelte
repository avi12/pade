<script lang="ts">
  import Icon from "@/lib/Icon.svelte";
  import { AddRootStatus, EMPTY_PATH_PROBE } from "@/lib/types";
  import type { AddRootOutcome, TaggedPathProbe } from "@/lib/types";
  import PathCombobox from "@/panels/picker/PathCombobox.svelte";
  import { assessRootPath } from "@/panels/picker/root-path";
  import { open as openDialog } from "@tauri-apps/plugin-dialog";

  // The add-root row: a typed path (live validation + directory autocomplete)
  // or the native folder picker. As the path is typed, the backend is probed
  // (debounced) for what it is on disk and for child directories to suggest -
  // so the row says whether it will add or create the folder, and rejects a
  // file, before anything is persisted.
  const { roots, onadd }: {
    roots: string[];
    onadd: (path: string, options: {
      create: boolean;
    }) => Promise<AddRootOutcome>;
  } = $props();

  let newRoot = $state("");
  // The latest probe result, tagged with the path it was computed for so the
  // "exists" flags are only trusted once they describe the current text. Filled
  // by the PathCombobox (which owns the debounced probe + the autocomplete).
  let probe = $state<TaggedPathProbe>({
    path: "",
    result: EMPTY_PATH_PROBE
  });
  let elInput = $state<HTMLInputElement | null>(null);

  const assessment = $derived(
    assessRootPath({
      typed: newRoot,
      roots,
      probe
    })
  );
  const addLabel = $derived.by(() => {
    if (assessment.willCreate) {
      return "Create & add";
    }

    return "Add root";
  });

  /** Focus the add-root field - how the quick-start form's "New root folder…"
   *  menu option lands the user here. Focus itself scrolls the field into view. */
  export function focusAddRoot() {
    elInput?.focus();
  }

  // Add the path, creating the folder when it doesn't exist yet (the button
  // label already says which). Only an `added` outcome clears the field - a
  // file or a stray location is caught by the disabled gate + the live banner.
  // Shared by the form submit and the folder-picked-from-dialog paths.
  async function add(path: string, { create }: {
    create: boolean;
  }) {
    const outcome = await onadd(path, { create });
    if (outcome.status === AddRootStatus.enum.added) {
      newRoot = "";
    }
  }
</script>

<div class="addrow-wrap">
  <form
    class="addrow"
    onsubmit={async e => {
      e.preventDefault();

      if (assessment.addablePath) {
        await add(assessment.addablePath, { create: assessment.canCreate });
      }
    }}
  >
    <div class="combo">
      <!-- Typed path + directory autocomplete (shared PathCombobox); `framed`
           gives the standalone field its own chrome. Its bound `probe` drives
           the live status banner and the add gate below. -->
      <PathCombobox
        name="root"
        inputClass="framed"
        placeholder="C:\repositories  ·  paste or start typing a folder path"
        bind:value={newRoot}
        bind:probe
        bind:elInput
      />
    </div>
    <!-- Native folder picker (Tauri dialog) - nicer than pasting a path. It
         always returns an existing directory, so add it straight away. -->
    <button
      class="browse"
      onclick={async () => {
        const picked = await openDialog({
          directory: true,
          multiple: false
        });
        if (typeof picked === "string") {
          await add(picked, { create: false });
        }
      }}
      type="button"
    ><Icon name="folder" /> Browse…</button>
    <button class="add" disabled={!assessment.isAddable} type="submit">{addLabel}</button>
  </form>

  <!-- Live status of the typed path - updated as you type: invalid shape, a
       file, a duplicate root, an existing folder ready to add, or one PADE will
       create. Replaces a post-submit prompt so the guidance sits with the text. -->
  {#if assessment.status}
    <output class="status" data-tone={assessment.status.tone}>
      <Icon name={assessment.status.icon} size={16} />
      <span class="status-text">{assessment.status.text}</span>
    </output>
  {/if}
</div>

<style>
  .addrow-wrap {
    display: flex;
    flex-direction: column;
    gap: 8px;
  }

  .addrow {
    display: flex;
    flex-wrap: wrap;
    gap: 8px;
  }

  /* The typed-path field - holds the shared PathCombobox, which anchors its own
     autocomplete popover to itself, so the two stay aligned wherever it lands. */
  .combo {
    flex: 1;
    min-inline-size: 200px;
  }

  /* Live status banner - a tonal M3 surface whose colour states what will happen:
     critical (bad path / a file), warning (already a root), ok (exists), primary
     (will be created), neutral (checking). */
  .status {
    display: flex;
    gap: 7px;
    align-items: center;
    padding: 6px 9px;
    border-radius: var(--radius-small);
    font-weight: 600;
    font-size: 0.6875rem;
    line-height: 1.4;
    animation: rise 180ms var(--ease);

    &[data-tone="critical"] {
      background: var(--critical-wash);
      color: var(--critical);
    }

    &[data-tone="warning"] {
      background: var(--warning-wash);
      color: var(--warning);
    }

    &[data-tone="ok"] {
      background: var(--tertiary-wash);
      color: var(--tertiary);
    }

    &[data-tone="primary"] {
      background: var(--primary-container);
      color: var(--on-primary-container);
    }

    &[data-tone="neutral"] {
      background: var(--surface-2);
      color: var(--on-surface-variant);
    }

    .status-text {
      min-inline-size: 0;
    }
  }

  .add {
    font-size: 0.8125rem;
  }

  .browse {
    display: inline-flex;
    gap: 6px;
    align-items: center;
    padding: 10px 16px;
    border: 1px solid var(--outline);
    border-radius: var(--radius-medium);
    background: transparent;
    color: var(--on-surface);
    font: inherit;
    font-weight: 600;
    font-size: 13px;
    cursor: pointer;
    transition: background 150ms var(--ease);

    &:hover {
      background: var(--surface-2);
      filter: none;
    }
  }
</style>
