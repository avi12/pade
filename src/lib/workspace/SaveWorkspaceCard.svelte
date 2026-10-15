<script lang="ts">
  import { workspace } from "@/lib/bridge";
  import Icon from "@/lib/Icon.svelte";
  import { AddRootStatus } from "@/lib/types";
  import type { AddRootOutcome } from "@/lib/types";
  import { tooltip } from "@/lib/ui/truncation-tooltip";
  import { childPath } from "@/lib/util/paths";
  import { nameError, parseInput, ProjectName } from "@/lib/validate";
  import TempBadge from "@/lib/workspace/TempBadge.svelte";
  import { open as openDialog } from "@tauri-apps/plugin-dialog";
  import { untrack } from "svelte";

  // Save the throwaway workspace as a real project: name it and pick which saved
  // root receives it. One root saves straight into it; several list a Save button
  // each; none offers to create a root first (native folder picker) and then
  // saves into it. The switcher leads with this card for a temp project.
  const { label, roots, isMenuOpen, onsavetemp, onaddroot, onsaved, onrelabel }: {
    /** The workspace's shown name - the friendly prefill when it has one. */
    label: string;
    /** The saved project roots - destinations the workspace can be saved into. */
    roots: string[];
    /** The switcher just opened - the moment the name is prefilled. */
    isMenuOpen: boolean;
    /** Rename the workspace into `root` under `name`. Resolves once relocated. */
    onsavetemp: (target: {
      name: string;
      root: string;
    }) => Promise<void>;
    /** Add an existing folder as a project root - the no-roots save path. */
    onaddroot: (path: string) => Promise<AddRootOutcome>;
    /** The save landed - the switcher closes (the window relocates under it). */
    onsaved: () => void;
    /** Relabel the workspace in PADE only, leaving its folder where it is. */
    onrelabel: () => void;
  } = $props();

  // The auto-generated folder name a temp workspace starts with - never worth
  // prefilling, unlike a friendly label.
  const TEMP_FOLDER_NAME = /^temp-\d+$/;

  // Prefilled as the menu opens with the friendly label when one exists
  // (auto-name), never the raw temp-<stamp> folder name; anything typed sticks.
  let saveName = $state("");
  let isSaving = $state(false);
  let saveFailure = $state("");
  const saveNameIssue = $derived(nameError(saveName));
  const savableName = $derived(
    parseInput({
      schema: ProjectName,
      raw: saveName
    })
  );
  const canSave = $derived(savableName !== null && !isSaving);

  $effect(() => {
    if (!isMenuOpen) {
      return;
    }

    untrack(() => {
      const friendlyLabel = parseInput({
        schema: ProjectName,
        raw: label
      });
      const isLabelPrefillable = saveName === "" && friendlyLabel !== null && !TEMP_FOLDER_NAME.test(friendlyLabel);
      if (isLabelPrefillable) {
        saveName = friendlyLabel;
      }
    });
  });

  // Live disk check on each save target - the same collision guard the project
  // picker's new-project field uses (QuickStartSection). `workspace_rename` does
  // an `fs::rename`, which on Windows fails with a raw "os error 32/183" if the
  // target already exists; probe `<root>/<name>` up front so a colliding name
  // disables that root's Save with a plain reason instead of a cryptic failure.
  // Each probe is tagged with the name it described, so an out-of-order reply
  // never gates the current text.
  type SaveProbe = {
    name: string;
    root: string;
    exists: boolean;
  };
  let saveProbes = $state<SaveProbe[]>([]);
  $effect(() => {
    const name = savableName;
    if (name === null || roots.length === 0) {
      saveProbes = [];
      return;
    }

    async function probeRoots(targetName: string) {
      const probed = await Promise.all(
        roots.map(async root => {
          try {
            const probe = await workspace.probePath(
              childPath({
                parent: root,
                name: targetName
              })
            );
            return {
              name: targetName,
              root,
              exists: probe.isDir || probe.isFile
            };
          } catch {
            // An unreachable probe leaves the plain save path - the backend
            // still validates the final rename.
            return {
              name: targetName,
              root,
              exists: false
            };
          }
        })
      );
      // Only believe an answer that describes the name still in the field.
      if (targetName === savableName) {
        saveProbes = probed;
      }
    }
    probeRoots(name);
  });

  function isNameTakenIn(root: string) {
    return saveProbes.some(probe => probe.root === root && probe.name === savableName && probe.exists);
  }

  function canSaveInto(root: string) {
    return canSave && !isNameTakenIn(root);
  }

  function saveGoLabel(isTaken: boolean) {
    if (isTaken) {
      return "Name taken";
    }

    return "Save →";
  }

  // The live field issue, then the last failed save - either, both or none.
  const issues = $derived([saveNameIssue, saveFailure].filter(issue => Boolean(issue)));

  // Rename into `root`; the menu closes on success (the window relocates under
  // the saved path). A failure stays visible inside the card.
  async function saveTempInto(root: string) {
    if (!savableName || !canSave) {
      return;
    }

    isSaving = true;
    saveFailure = "";
    try {
      await onsavetemp({
        name: savableName,
        root
      });
      onsaved();
    } catch (error) {
      saveFailure = String(error);
    } finally {
      isSaving = false;
    }
  }

  // A listed root's Save: held back while the name is taken there, the same gate
  // that disables its button.
  async function saveIntoListedRoot(root: string) {
    if (!canSaveInto(root)) {
      return;
    }

    await saveTempInto(root);
  }

  // The no-roots path: pick an existing folder as the first root, then save
  // into it. The dialog always returns an existing directory, so any non-`added`
  // outcome is a real failure worth showing.
  async function createRootAndSave() {
    if (!canSave) {
      return;
    }

    const picked = await openDialog({
      directory: true,
      multiple: false
    });
    if (typeof picked !== "string") {
      return;
    }

    const outcome = await onaddroot(picked);
    if (outcome.status !== AddRootStatus.enum.added) {
      saveFailure = "That folder can't be used as a root";
      return;
    }

    await saveTempInto(picked);
  }

  // Enter saves into the PRIMARY root (roots[0]) - the same default
  // workspace_rename uses when no root is chosen - or opens the folder picker
  // when there are no roots yet. It respects the same gates as the primary
  // root's button (blank/busy/collision), so Enter can't save a name that
  // button shows disabled.
  async function saveViaEnter() {
    const primaryRoot = roots.at(0);
    if (primaryRoot === undefined) {
      await createRootAndSave();
      return;
    }

    await saveIntoListedRoot(primaryRoot);
  }
</script>

<div class="save-card">
  <div class="save-head">
    <TempBadge />
    <span class="save-title">Save this workspace</span>
    <button
      class="save-relabel"
      {@attach tooltip("Rename it in PADE only - the folder keeps its path")}
      onclick={onrelabel}
      type="button"
    ><Icon name="pencil" size={13} /> Relabel</button>
  </div>
  <p class="save-hint">Name it and pick a root to keep it beyond this session</p>
  <form
    class="save-form" onsubmit={async e => {
      e.preventDefault();
      await saveViaEnter();
    }}>
    <input
      class="save-name"
      aria-label="Project name"
      autocomplete="off"
      placeholder="project-name"
      spellcheck="false"
      bind:value={saveName}
    />
    {#each issues as issue, i (i)}
      <output class="save-issue">{issue}</output>
    {/each}
    {#if roots.length === 0}
      <button
        class="save-root"
        disabled={!canSave}
        onclick={createRootAndSave}
        type="button"
      >
        <span class="save-root-icon" aria-hidden="true"><Icon name="folderPlus" size={15} /></span>
        <span class="save-root-path">Choose a folder for your projects…</span>
        <span class="save-go">Create root & save →</span>
      </button>
    {:else}
      <div class="save-roots">
        {#each roots as root (root)}
          {@const isTaken = isNameTakenIn(root)}
          <button
            class="save-root"
            class:save-root-collides={isTaken}
            disabled={!canSaveInto(root)}
            onclick={async () => await saveIntoListedRoot(root)}
            type="button"
          >
            <span class="save-root-icon" aria-hidden="true"><Icon name="folder" size={15} /></span>
            <span class="save-root-path">{root}</span>
            <span class="save-go">{saveGoLabel(isTaken)}</span>
          </button>
        {/each}
      </div>
    {/if}
  </form>
</div>

<style>
  /* "Save this workspace" - the temp-promotion card leading the menu. */
  .save-card {
    display: flex;
    flex-direction: column;
    gap: 8px;
    margin-block: 1px 6px;
    margin-inline: 2px;
    padding: 10px;
    border: 1px solid var(--outline);
    border-radius: var(--radius-medium);
    background: var(--surface-1);

    /* The <form> exists only to make Enter submit; it must not become a flex item
       or it would swallow the card's column gap around the field and roots. */
    .save-form {
      display: contents;
    }

    .save-head {
      display: flex;
      gap: 6px;
      align-items: center;
    }

    .save-title {
      font-weight: 700;
      font-size: 12px;
    }

    /* Rename-in-PADE-only, tucked at the head's end: a quiet text button, so the
       card's primary action stays Save. */
    .save-relabel {
      display: inline-flex;
      gap: 4px;
      align-items: center;
      margin-inline-start: auto;
      padding-block: 3px;
      padding-inline: 8px;
      border: none;
      border-radius: var(--radius-full);
      background: transparent;
      color: var(--on-surface-variant);
      font: inherit;
      font-weight: 600;
      font-size: 11px;
      cursor: pointer;
      transition:
        background 150ms var(--ease),
        color 150ms var(--ease);

      &:hover {
        background: var(--surface-3);
        color: var(--on-surface);
      }
    }

    .save-hint {
      margin: 0;
      color: var(--on-surface-variant);
      font-size: 10px;
      line-height: 1.4;
    }

    .save-name {
      inline-size: 100%;
      padding-block: 7px;
      padding-inline: 8px;
      border: 1px solid var(--outline);
      border-radius: var(--radius-small);
      background: var(--surface-2);
      color: var(--on-surface);
      outline: none;
      font-family: var(--font-monospace);
      font-weight: 600;
      font-size: 12px;

      &:focus-visible {
        border-color: var(--primary);
      }
    }

    .save-issue {
      color: var(--critical);
      font-weight: 600;
      font-size: 10.5px;
      line-height: 1.4;
    }

    .save-roots {
      display: flex;
      flex-direction: column;
      gap: 3px;
    }

    .save-root {
      display: flex;
      gap: 8px;
      align-items: center;
      inline-size: 100%;
      padding-block: 7px;
      padding-inline: 8px;
      border-radius: var(--radius-small);
      background: transparent;
      color: var(--on-surface);
      font: inherit;
      text-align: start;
      cursor: pointer;
      transition: color 120ms var(--ease), background 120ms var(--ease);

      &:hover:not(:disabled) {
        background: var(--primary-container);
        color: var(--on-primary-container);
      }

      &:disabled {
        opacity: 55%;
        cursor: default;
      }

      .save-root-icon {
        display: inline-flex;
        flex: none;
        color: var(--on-surface-variant);
      }

      .save-root-path {
        flex: 1;
        overflow: hidden;
        min-inline-size: 0;
        font-family: var(--font-monospace);
        font-weight: 600;
        font-size: 11px;
        text-overflow: ellipsis;
        white-space: nowrap;
      }

      .save-go {
        flex: none;
        color: var(--primary);
        font-weight: 700;
        font-size: 10px;
      }

      &.save-root-collides .save-go {
        color: var(--critical);
      }

      &:hover:not(:disabled) .save-go {
        color: var(--on-primary-container);
      }
    }
  }
</style>
