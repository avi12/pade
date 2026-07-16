<script lang="ts">
  import { workspace } from "@/lib/bridge";
  import Icon from "@/lib/Icon.svelte";
  import type { IconName } from "@/lib/Icon.svelte";
  import { collapseRow } from "@/lib/motion";
  import { AddRootStatus } from "@/lib/types";
  import type { AddRootOutcome, Ide, PathProbe, ProjectEntry } from "@/lib/types";
  import { FolderPath, parseInput } from "@/lib/validate";
  import type { WorkspaceLifecycle } from "@/panels/picker/lifecycle.svelte";
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

  const emptyProbe: PathProbe = {
    isDir: false,
    isFile: false,
    parentExists: false,
    suggestions: []
  };

  let newRoot = $state("");
  // The latest probe result, tagged with the path it was computed for so the
  // "exists" flags are only trusted once they describe the current text.
  let probe = $state<{
    path: string;
    result: PathProbe;
  }>({
    path: "",
    result: emptyProbe
  });
  let listOpen = $state(false);
  // Keyboard-highlighted suggestion, or -1 when pointer/typing drives the list.
  let activeIndex = $state(-1);
  let inputEl = $state<HTMLInputElement | null>(null);
  let listEl = $state<HTMLUListElement | null>(null);

  // The OS path separator — appended when a highlighted suggestion is Tab-accepted
  // so the user can keep drilling into its sub-folders. The webview has no
  // `path.sep`, so read the platform the way OnLaunchSection already does.
  const pathSeparator = navigator.userAgent.includes("Windows") ? "\\" : "/";

  const trimmedRoot = $derived(newRoot.trim());
  const hasValue = $derived(trimmedRoot.length > 0);
  const alreadyRoot = $derived(
    roots.some(root => root.toLowerCase() === trimmedRoot.toLowerCase())
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
  // Never echo the exact folder already typed back as a suggestion.
  const suggestions = $derived(
    probe.result.suggestions.filter(dir => dir.toLowerCase() !== probe.path.toLowerCase())
  );

  const willCreate = $derived(canCreate && !alreadyRoot);
  const addLabel = $derived(willCreate ? "Create & add" : "Add root");
  const addDisabled = $derived(
    !probeSettled || alreadyRoot || typedPathIsFile || invalidLocation
  );
  const listVisible = $derived(listOpen && suggestions.length > 0);
  const activeId = $derived(activeIndex >= 0 ? `root-suggestion-${activeIndex}` : undefined);

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

  // Debounced backend probe: what the typed path is on disk (and whether its
  // parent exists) + directory completions. Empty field clears it immediately.
  $effect(() => {
    const path = trimmedRoot;
    if (path.length === 0) {
      probe = {
        path: "",
        result: emptyProbe
      };
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

  // Adopt a suggested directory — shared by the listbox rows and the Enter key,
  // so it stays a named function.
  function pick(dir: string) {
    newRoot = dir;
    listOpen = false;
    activeIndex = -1;
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
      listOpen = false;
    }
  }
</script>

<section class="roots">
  <h2>Root folders</h2>
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
        <input
          bind:this={inputEl}
          aria-activedescendant={activeId}
          aria-autocomplete="list"
          aria-controls="root-suggestions"
          aria-expanded={listVisible}
          autocomplete="off"
          onblur={() => (listOpen = false)}
          onfocus={() => (listOpen = true)}
          oninput={() => {
            listOpen = true; activeIndex = -1;
          }}
          onkeydown={e => {
            if (!listVisible) {
              return;
            }

            if (e.key === "ArrowDown") {
              e.preventDefault();
              activeIndex = (activeIndex + 1) % suggestions.length;
            } else if (e.key === "ArrowUp") {
              e.preventDefault();
              activeIndex = activeIndex <= 0 ? suggestions.length - 1 : activeIndex - 1;
            } else if (e.key === "Enter" && activeIndex >= 0) {
              e.preventDefault();
              pick(suggestions[activeIndex]);
            } else if (e.key === "Tab" && activeIndex >= 0) {
              // Accept the highlighted folder and append a separator instead of
              // moving focus, then keep the list open so the debounced probe
              // immediately starts completing that folder's sub-folders.
              e.preventDefault();
              newRoot = suggestions[activeIndex] + pathSeparator;
              activeIndex = -1;
              listOpen = true;
            } else if (e.key === "Escape") {
              listOpen = false; activeIndex = -1;
            }
          }}
          placeholder="C:\repositories  ·  paste or start typing a folder path"
          role="combobox"
          spellcheck="false"
          type="text"
          bind:value={newRoot}
        />
        <!-- Autocomplete listbox as a top-layer popover, anchored to the field
             with CSS anchor positioning: the top layer escapes the picker's own
             scroll clip, and flip-block flips it above the field near the viewport
             bottom, so it can never clip through the edge. Shown/hidden via
             showPopover/hidePopover from the `listVisible` effect (a typeahead
             opens on typing, not on a popovertarget click). -->
        <ul
          bind:this={listEl}
          id="root-suggestions"
          class="suggestions"
          popover="manual"
          role="listbox"
        >
          {#each suggestions as dir, index (dir)}
            <li>
              <button
                id={`root-suggestion-${index}`}
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
    <div class="root">
      <div class="root-head">
        <code class="rootpath">{root}</code>
        <button
          class="remove"
          aria-label="Remove root"
          data-tooltip="Remove root"
          onclick={async () => await onremove(root)}
        ><Icon name="close" size={14} /></button>
      </div>
      <ul class="projects">
        {#each projectsByRoot[root] ?? [] as project (project.path)}
          <li class="row" out:collapseRow>
            <button class="project" onclick={() => onopen({ path: project.path })}>
              <span class="pname">{project.name}</span>
              {#if project.isGit}
                <span class="git">git</span>
              {/if}
            </button>
            <RowMenu {ides} {lifecycle} path={project.path} scope="root" />
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

  /* The typed-path field. Its input is the anchor the autocomplete popover binds
     to, so the two stay aligned wherever the field lands. */
  .combo {
    flex: 1;
    min-inline-size: 200px;

    input {
      inline-size: 100%;
      font-family: var(--font-monospace);
      font-size: 13px;
      anchor-name: --root-combo;
    }
  }

  /* Directory autocomplete — a card of child folders shown as a top-layer popover
     anchored to the field. The top layer lifts it out of the picker's own scroll
     container (so it never clips), `anchor-size(width)` matches the field's width,
     and `flip-block` flips it above the field near the viewport bottom. The reveal
     transition and `margin: 0` come from the shared `[popover]` base. */
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
    position-anchor: --root-combo;
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

  /* Live status banner — a tonal M3 surface whose colour states what will happen:
     critical (bad path / a file), warning (already a root), ok (exists), primary
     (will be created), neutral (checking). */
  .status {
    display: flex;
    gap: 8px;
    align-items: center;
    padding: 8px 12px;
    border-radius: var(--radius-medium);
    font-weight: 600;
    font-size: 13px;
    line-height: 1.4;
    animation: banner-in 180ms var(--ease);

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

  @keyframes banner-in {
    from {
      opacity: 0%;
      transform: translateY(-4px);
    }
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
    gap: 10px;
    align-items: center;
  }

  .rootpath {
    padding: 4px 10px;
    border-radius: var(--radius-small);
    background: var(--surface-2);
    color: var(--on-surface);
    font-family: var(--font-monospace);
    font-size: 12px;
  }

  .remove {
    display: inline-flex;
    flex: none;
    justify-content: center;
    align-items: center;
    block-size: 24px;
    inline-size: 24px;
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
