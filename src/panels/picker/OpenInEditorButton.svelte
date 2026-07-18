<script lang="ts">
  import { ide } from "@/lib/bridge";
  import Icon from "@/lib/Icon.svelte";
  import type { Ide } from "@/lib/types";

  // A picker row's "open this project in your editor" button. It stays a plain,
  // static button — the best-fit editor for the project's kind is worked out only
  // when clicked (one `ide.suggest`, the same rules → fallback → coverage ranking
  // the workspace's "Open in <editor>" button uses), never per row on render, so a
  // rootful of rows never blocks the UI. Shown whenever a GUI editor is installed
  // (a console editor needs a TTY the picker lacks, so it launches from the
  // workspace). Shared by the Recent and Root rows (its one home, so the
  // editor-open action isn't also duplicated in the row's ⋯ menu).
  const { path, name, ides }: {
    path: string;
    name: string;
    ides: Ide[];
  } = $props();

  const hasGuiEditor = $derived(ides.some(editor => !editor.terminal));
  // True only while a click's best-fit lookup is in flight — the button reflects
  // it (disabled + dimmed) so the ~second `ide.suggest` takes doesn't read as a
  // dead click, and a second click can't stack another lookup.
  let busy = $state(false);

  async function suggestOrFallback(): Promise<Ide[]> {
    try {
      return await ide.suggest(path);
    } catch {
      return ides;
    }
  }

  async function openInEditor() {
    if (busy) {
      return;
    }

    busy = true;
    try {
      const ranked = await suggestOrFallback();
      const editor = ranked.find(candidate => !candidate.terminal);
      if (editor) {
        void ide.open({
          command: editor.command,
          path
        });
      }
    } finally {
      busy = false;
    }
  }
</script>

{#if hasGuiEditor}
  <button
    class="open-ide"
    class:busy
    aria-label={`Open ${name} in editor`}
    data-tooltip="Open in editor"
    disabled={busy}
    onclick={openInEditor}
  ><Icon name="code" size={16} /></button>
{/if}

<style>
  /* Per-row quick action: open the project in its best-fit editor (resolved on
     click) — the visible twin of the workspace topbar's "Open in <editor>" button,
     so a project opens in your editor without going through the ⋯ menu. */
  .open-ide {
    display: inline-flex;
    flex: none;
    justify-content: center;
    align-items: center;
    block-size: 32px;
    inline-size: 32px;
    padding: 0;
    border-radius: 999px;
    background: transparent;
    color: var(--on-surface-variant);
    cursor: pointer;
    transition:
      background 150ms var(--ease),
      opacity 150ms var(--ease);

    &:hover {
      background: var(--surface-3);
      filter: none;
    }

    &.busy {
      opacity: 55%;
      cursor: default;
    }
  }
</style>
