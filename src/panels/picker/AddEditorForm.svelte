<script lang="ts">
  import Icon from "@/lib/Icon.svelte";
  import { FolderPath, parseInput } from "@/lib/validate";
  import { open as openDialog } from "@tauri-apps/plugin-dialog";

  // The inline "Add editor…" form: an executable path (typed or browsed),
  // validated and persisted through `onaddeditor`. The outcome is reported back
  // - `onadded` with the new editor's label, `onrejected` with the reason - and
  // `onedit` fires whenever the path changes, so a stale message can clear.
  const { onaddeditor, onadded, onrejected, onedit, oncancel }: {
    onaddeditor: (path: string) => Promise<{
      label: string;
    } | {
      error: string;
    }>;
    onadded: (label: string) => void;
    onrejected: (reason: string) => void;
    onedit: () => void;
    oncancel: () => void;
  } = $props();

  let draft = $state("");
</script>

<form
  class="editor-addition-form"
  onsubmit={async e => {
    e.preventDefault();
    // The path is a trust boundary - trim/length-cap it before it leaves the UI.
    const path = parseInput({
      schema: FolderPath,
      raw: draft
    });
    if (path === null) {
      onrejected("Enter the full path to an editor executable");
      return;
    }

    const result = await onaddeditor(path);
    if ("error" in result) {
      onrejected(result.error);
      return;
    }

    onadded(result.label);
  }}
>
  <div class="editor-location text-field-plate">
    <span class="editor-location-icon" aria-hidden="true"><Icon name="folder" /></span>
    <label class="visually-hidden" for="editor-location-input">Path to editor executable</label>
    <input
      id="editor-location-input"
      class="editor-location-input"
      autocomplete="off"
      oninput={() => onedit()}
      placeholder="C:\path\to\editor.exe"
      spellcheck="false"
      bind:value={draft}
    />
    <button
      class="editor-browse"
      onclick={async () => {
        const picked = await openDialog({
          multiple: false,
          title: "Locate an editor’s executable"
        });
        if (typeof picked === "string") {
          draft = picked;
          onedit();
        }
      }}
      type="button"
    >Browse…</button>
  </div>
  <div class="editor-addition-actions">
    <button class="editor-confirm" type="submit">Add editor</button>
    <button class="editor-cancel" onclick={() => oncancel()} type="button">Cancel</button>
    <span class="editor-addition-hint">Select the editor’s executable</span>
  </div>
</form>

<style>
  .editor-addition-form {
    display: flex;
    flex-direction: column;
    gap: 8px;
  }

  /* Path field - the shared field plate: mono input with a folder lead and a
     ghost Browse…. */
  .editor-location {
    --field-plate-padding-inline-end: 4px;

    display: flex;
    gap: 4px;
    align-items: center;

    .editor-location-icon {
      display: inline-flex;
      flex: none;
      color: var(--on-surface-variant);
    }
  }

  .editor-location-input {
    flex: 1;
    min-inline-size: 0;
    font-family: var(--font-monospace);
  }

  .editor-browse {
    flex: none;
    padding: 6px 10px;
    border: none;
    border-radius: var(--radius-small);
    background: transparent;
    color: var(--on-surface-variant);
    font: inherit;
    font-weight: 600;
    font-size: 0.6875rem;
    cursor: pointer;
    transition:
      background 150ms var(--ease),
      color 150ms var(--ease);

    &:hover {
      background: var(--surface-3);
      color: var(--on-surface);
    }
  }

  .editor-addition-actions {
    display: flex;
    gap: 8px;
    align-items: center;
  }

  .editor-addition-hint {
    margin-inline-start: auto;
    color: var(--on-surface-variant);
    font-size: 0.6875rem;
  }

  .editor-confirm {
    padding: 7px 14px;
    border: none;
    border-radius: var(--radius-small);
    background: var(--primary);
    color: var(--on-primary);
    font: inherit;
    font-weight: 700;
    font-size: 0.75rem;
    cursor: pointer;
    transition: filter 150ms var(--ease);

    &:hover {
      filter: brightness(1.06);
    }
  }

  .editor-cancel {
    padding: 8px 14px;
    border: none;
    border-radius: var(--radius-small);
    background: transparent;
    color: var(--on-surface-variant);
    font: inherit;
    font-weight: 600;
    font-size: 0.75rem;
    cursor: pointer;
    transition: background 150ms var(--ease);

    &:hover {
      background: var(--surface-3);
    }
  }
</style>
