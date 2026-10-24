<script lang="ts">
  import Icon from "@/lib/Icon.svelte";
  import { EventType, onMessage } from "@/lib/ipc/messaging";
  import { EMPTY_PATH_PROBE } from "@/lib/types";
  import type { DragPosition, OpenTarget, TaggedPathProbe } from "@/lib/types";
  import { isDragOverElement } from "@/lib/ui/drag-position";
  import { FolderPath, parseInput } from "@/lib/validate";
  import PathCombobox from "@/panels/picker/PathCombobox.svelte";
  import type { UnlistenFn } from "@tauri-apps/api/event";
  import { open as openDialog } from "@tauri-apps/plugin-dialog";
  import { onDestroy, onMount } from "svelte";

  // Get started's Open local tab: open an existing folder - typed (with live
  // validation + directory autocomplete), picked from the OS dialog, or dropped
  // from Explorer / an IDE. `isActive` says whether this tab is the open one.
  const { isActive, onopen }: {
    isActive: boolean;
    onopen: (target: OpenTarget) => void;
  } = $props();

  let localPath = $state("");
  let elLocalInput = $state<HTMLInputElement | null>(null);
  // True while an OS drag hovers the Local field - the drop-affordance state.
  let isDropReady = $state(false);

  // Does a native drag sit over the Local tab's folder field? The panels
  // overlay each other, so the tab gate matters as much as the geometry (the
  // clone URL input occupies the same spot).
  function isOverLocalField(position: DragPosition) {
    const row = elLocalInput?.closest(".new-project-location");
    if (!isActive || !row) {
      return false;
    }

    return isDragOverElement({
      element: row,
      position
    });
  }
  // The latest probe, tagged with the path it described - only a settled probe
  // (disk knowledge about the *current* text) gates the button or complains.
  // Filled by the PathCombobox, which owns the debounced probe + autocomplete.
  let localProbe = $state<TaggedPathProbe>({
    path: "",
    result: EMPTY_PATH_PROBE
  });
  const trimmedLocal = $derived(localPath.trim());
  const localSettled = $derived(trimmedLocal.length > 0 && localProbe.path === trimmedLocal);
  const localIsDirectory = $derived(localSettled && localProbe.result.isDir);
  const localError = $derived.by(() => {
    if (localSettled && !localProbe.result.isDir) {
      return "That folder doesn’t exist";
    }

    return null;
  });

  // A folder dragged from Explorer / an IDE onto the Local field fills its
  // path (Tauri's native DnD events carry the absolute paths the web API
  // withholds). The `over` stream drives the field's drop affordance.
  let unlistenDragOver: UnlistenFn | undefined;
  let unlistenDragDrop: UnlistenFn | undefined;
  let unlistenDragLeave: UnlistenFn | undefined;

  onMount(async () => {
    unlistenDragOver = await onMessage(EventType.dragOver, ({ data: { position } }) => {
      isDropReady = isOverLocalField(position);
    });
    unlistenDragLeave = await onMessage(EventType.dragLeave, () => (isDropReady = false));
    unlistenDragDrop = await onMessage(EventType.dragDrop, ({ data: { paths, position } }) => {
      const [dropped] = paths;
      const landsOnField = dropped !== undefined && isOverLocalField(position);
      isDropReady = false;

      if (!landsOnField) {
        return;
      }

      const path = parseInput({
        schema: FolderPath,
        raw: dropped
      });
      if (path === null) {
        return;
      }

      localPath = path;
      elLocalInput?.focus();
    });
  });

  onDestroy(() => {
    unlistenDragOver?.();
    unlistenDragDrop?.();
    unlistenDragLeave?.();
  });
</script>

<form
  class="panel"
  onsubmit={e => {
    e.preventDefault();
    const path = parseInput({
      schema: FolderPath,
      raw: localPath
    });
    if (!path || !localIsDirectory) {
      return;
    }

    onopen({ path });
  }}
>
  <div class="new-project-field">
    <label class="new-project-label" for="local-path">Project folder</label>
    <div class="new-project-location" class:drop-ready={isDropReady}>
      <span class="new-project-location-icon" aria-hidden="true"><Icon name="folder" /></span>
      <PathCombobox
        id="local-path"
        name="local"
        placeholder="C:\repositories\my-app  ·  or drop a folder here"
        bind:value={localPath}
        bind:probe={localProbe}
        bind:elInput={elLocalInput}
      />
      <button
        class="browse"
        onclick={async () => {
          const picked = await openDialog({
            directory: true,
            multiple: false
          });
          if (typeof picked === "string") {
            localPath = picked;
          }
        }}
        type="button"
      >Browse…</button>
    </div>
    {#if localError}
      <output class="field-error">{localError}</output>
    {/if}
  </div>

  <button class="go" disabled={!localIsDirectory} type="submit">Open project</button>
</form>

<style>
  .browse {
    flex: none;
    padding: 6px 12px;
    border: 1px solid var(--outline);
    border-radius: var(--radius-small);
    background: transparent;
    color: var(--on-surface);
    font: inherit;
    font-weight: 600;
    font-size: 0.75rem;
    cursor: pointer;
    transition:
      color 150ms var(--ease),
      border-color 150ms var(--ease);

    &:hover {
      border-color: var(--primary);
      color: var(--primary);
      filter: none;
    }
  }
</style>
