<script lang="ts">
  import Icon from "@/lib/Icon.svelte";
  import type { IconName } from "@/lib/Icon.svelte";
  import { collapseRow } from "@/lib/motion";
  import { normalizePath } from "@/lib/paths";
  import { AddRootStatus, emptyPathProbe } from "@/lib/types";
  import type { AddRootOutcome, Ide, ProjectEntry, TaggedPathProbe } from "@/lib/types";
  import { FolderPath, parseInput } from "@/lib/validate";
  import type { WorkspaceLifecycle } from "@/panels/picker/lifecycle.svelte";
  import OpenInEditorButton from "@/panels/picker/OpenInEditorButton.svelte";
  import PathCombobox from "@/panels/picker/PathCombobox.svelte";
  import RowMenu from "@/panels/picker/RowMenu.svelte";
  import { open as openDialog } from "@tauri-apps/plugin-dialog";

  // Root folders: add one (a typed path with live validation + directory
  // autocomplete, or the native folder picker), remove one, and browse the
  // projects detected inside each. Root persistence and the per-root scan stay
  // with the parent (single settings owner) via onadd / onremove. As the path is
  // typed, the backend is probed (debounced) for what it is on disk and for child
  // directories to suggest — so the row says whether it will add or create the
  // folder, and rejects a file, before anything is persisted.
  const { roots, projectsByRoot, ides, lifecycle, onopen, onadd, onremove }: {
    roots: string[];
    projectsByRoot: Record<string, ProjectEntry[]>;
    ides: Ide[];
    lifecycle: WorkspaceLifecycle;
    onopen: (target: { path: string }) => void;
    onadd: (path: string, options: {
      create: boolean;
    }) => Promise<AddRootOutcome>;
    onremove: (path: string) => Promise<void>;
  } = $props();

  // "1 project" / "6 projects" — count and matching noun as one string, so no
  // template whitespace can split them.
  function projectCountLabel(count: number): string {
    return `${count} project${count === 1 ? "" : "s"}`;
  }

  let newRoot = $state("");
  // The latest probe result, tagged with the path it was computed for so the
  // "exists" flags are only trusted once they describe the current text. Filled
  // by the PathCombobox (which owns the debounced probe + the autocomplete).
  let probe = $state<TaggedPathProbe>({
    path: "",
    result: emptyPathProbe
  });
  let inputEl = $state<HTMLInputElement | null>(null);

  const trimmedRoot = $derived(newRoot.trim());
  const hasValue = $derived(trimmedRoot.length > 0);
  // A path that differs from an existing root only by case or a trailing separator
  // is the same folder — `C:\repositories\` is already `C:\repositories`.
  const alreadyRoot = $derived(
    roots.some(root => normalizePath(root) === normalizePath(trimmedRoot))
  );
  // The probe is async + debounced; only believe its flags once they describe the
  // path currently in the field.
  const probeSettled = $derived(hasValue && probe.path === trimmedRoot);
  const folderExists = $derived(probeSettled && probe.result.isDir);
  const typedPathIsFile = $derived(probeSettled && probe.result.isFile);
  // A not-yet-created folder is addable only when its parent is a real directory —
  // an existence check, not a regex, tells a real location from a stray string.
  const canCreate = $derived(probeSettled && !folderExists && !typedPathIsFile && probe.result.parentExists);
  const invalidLocation = $derived(
    probeSettled && !folderExists && !typedPathIsFile && !probe.result.parentExists
  );

  const willCreate = $derived(canCreate && !alreadyRoot);
  const addLabel = $derived(willCreate ? "Create & add" : "Add root");
  const addDisabled = $derived(
    !probeSettled || alreadyRoot || typedPathIsFile || invalidLocation
  );

  type StatusTone = "critical" | "warning" | "ok" | "primary" | "neutral";
  const status = $derived.by((): {
    tone: StatusTone;
    icon: IconName;
    text: string;
  } | null => {
    if (!hasValue) {
      return null;
    }

    if (alreadyRoot) {
      return {
        tone: "warning",
        icon: "folder",
        text: "Already a root folder."
      };
    }

    if (!probeSettled) {
      return {
        tone: "neutral",
        icon: "search",
        text: "Checking…"
      };
    }

    if (typedPathIsFile) {
      return {
        tone: "critical",
        icon: "alert",
        text: "That path is a file, not a folder."
      };
    }

    if (folderExists) {
      return {
        tone: "ok",
        icon: "check",
        text: "Folder exists — ready to add."
      };
    }

    if (canCreate) {
      return {
        tone: "primary",
        icon: "folderPlus",
        text: "Folder doesn’t exist — PADE will create it."
      };
    }

    return {
      tone: "critical",
      icon: "alert",
      text: "That location doesn’t exist — enter a full folder path."
    };
  });

  /** Focus the add-root field — how the quick-start form's "New root folder…"
   *  menu option lands the user here. Focus itself scrolls the field into view. */
  export function focusAddRoot() {
    inputEl?.focus();
  }

  // Add the settled path, creating the folder when it doesn't exist yet (the
  // button label already says which). Only an `added` outcome clears the field —
  // a file or a stray location is caught by the disabled gate + the live banner.
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

<section class="roots">
  <h2>Root folders</h2>
  <p class="hint">
    A root is a folder your projects live in — a home base for starting new work.
    Add one and PADE lists every project inside it below; open any in a click, and
    projects you create or clone there show up here automatically.
  </p>
  <div class="addrow-wrap">
    <form
      class="addrow" onsubmit={async e => {
        e.preventDefault();
        const path = parseInput({
          schema: FolderPath,
          raw: newRoot
        });
        if (!path || addDisabled) {
          return;
        }

        await add(path, { create: canCreate });
      }}>
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
          bind:inputElement={inputEl}
        />
      </div>
      <!-- Native folder picker (Tauri dialog) — nicer than pasting a path. It
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
      <button class="add" disabled={addDisabled} type="submit">{addLabel}</button>
    </form>

    <!-- Live status of the typed path — updated as you type: invalid shape, a
         file, a duplicate root, an existing folder ready to add, or one PADE will
         create. Replaces a post-submit prompt so the guidance sits with the text. -->
    {#if status}
      <output class="status" data-tone={status.tone}>
        <Icon name={status.icon} size={16} />
        <span class="status-text">{status.text}</span>
      </output>
    {/if}
  </div>

  {#each roots as root (root)}
    {@const projects = projectsByRoot[root] ?? []}
    <div class="root" out:collapseRow>
      <div class="root-head">
        <span class="root-ico" aria-hidden="true"><Icon name="folder" size={15} /></span>
        <code class="rootpath">{root}</code>
        {#if projects.length > 0}
          <span class="root-count">{projectCountLabel(projects.length)}</span>
        {/if}
        <button
          class="remove"
          aria-label="Remove root"
          data-tooltip="Remove root"
          onclick={async () => await onremove(root)}
        ><Icon name="close" size={14} /></button>
      </div>
      <ul class="projects">
        {#each projects as project (project.path)}
          <li class="row" out:collapseRow>
            <button class="project" onclick={() => onopen({ path: project.path })}>
              <span class="pname">{project.name}</span>
              {#if project.isGit}
                <span class="git">git</span>
              {/if}
            </button>
            <OpenInEditorButton name={project.name} {ides} path={project.path} />
            <RowMenu {lifecycle} path={project.path} scope="root" />
          </li>
        {:else}
          <li class="none">No projects found in this folder.</li>
        {/each}
      </ul>
    </div>
  {/each}
</section>

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

  /* The typed-path field — holds the shared PathCombobox, which anchors its own
     autocomplete popover to itself, so the two stay aligned wherever it lands. */
  .combo {
    flex: 1;
    min-inline-size: 200px;
  }

  /* Live status banner — a tonal M3 surface whose colour states what will happen:
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

  .roots .root {
    display: flex;
    flex-direction: column;
    gap: 6px;
  }

  .root-head {
    display: flex;
    gap: 8px;
    align-items: center;
  }

  .root-ico {
    display: inline-flex;
    flex: none;
    color: var(--on-surface-variant);
  }

  .rootpath {
    padding: 4px 10px;
    border-radius: var(--radius-small);
    background: var(--surface-2);
    color: var(--on-surface);
    font-family: var(--font-monospace);
    font-size: 12px;
  }

  /* How many projects the root holds — frames the list below as "what's inside". */
  .root-count {
    color: var(--on-surface-variant);
    font-weight: 600;
    font-size: 11px;
    font-variant-numeric: tabular-nums;
    white-space: nowrap;
  }

  .remove {
    display: inline-flex;
    flex: none;
    justify-content: center;
    align-items: center;
    block-size: 24px;
    inline-size: 24px;

    /* Pushed to the row's end, past the path + count. */
    margin-inline-start: auto;
    padding: 0;
    border-radius: 999px;
    background: var(--surface-2);
    color: var(--on-surface-variant);

    &:hover {
      background: var(--critical-wash);
      color: var(--critical);
      filter: none;
    }
  }

  /* The per-row "open in editor" button lives in picker/OpenInEditorButton.svelte
     (shared with the Recent rows). */

  .projects {
    display: flex;
    flex-direction: column;
    gap: 6px;
    margin: 0;
    padding: 0;
    list-style: none;
  }

  /* Detected project — tonal card; lifts into primary-container on hover. */
  .project {
    display: flex;
    gap: 10px;
    align-items: center;
    inline-size: 100%;
    padding: 11px 13px;
    border-radius: var(--radius-medium);
    background: var(--surface-2);
    color: var(--on-surface);
    text-align: start;
    transition:
      background 150ms var(--ease),
      color 150ms var(--ease);

    &:hover {
      background: var(--primary-container);
      color: var(--on-primary-container);
      filter: none;
    }

    .pname {
      font-weight: 600;
      font-size: 14px;
    }

    .git {
      color: var(--tertiary);
      font-weight: 700;
      font-size: 9px;
      letter-spacing: 0.06em;
      text-transform: uppercase;
    }
  }
</style>
