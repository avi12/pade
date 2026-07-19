<script lang="ts">
  import { ide, os } from "@/lib/bridge";
  import Icon from "@/lib/Icon.svelte";
  import { ideBrand, ideIcon } from "@/lib/ide-icon";
  import { chooseEditor, editorsFor, refreshEditors } from "@/lib/stores/editors.svelte";
  import type { Ide } from "@/lib/types";

  // Opens the active project in an external editor. The ranked list comes from
  // the shared editors store (SSOT — the same list the Change Feed's reveal
  // reads), whose backend `ide_suggest` puts the project's editor first: an
  // explicit pick from this menu, else the best auto-detected fit. The split
  // button's primary action opens that editor directly, and the caret always
  // drops the full list, ending with File Explorer. A console editor
  // (Neovim/Vim/Helix) can't run detached, so it's handed to the parent to open
  // in a PADE terminal tab instead of through the OS.
  const { onterminaleditor, project, cwd }: {
    onterminaleditor: (editor: Ide) => void;
    // The open project — the key the shared editor ranking is resolved under,
    // shared with the Change Feed so the two surfaces can't drift.
    project: string;
    // The directory the launcher opens: the active session's worktree when one
    // is focused, else the project itself. Also the target of "Reveal in file
    // explorer", so the selector always has that final action.
    cwd: string;
  } = $props();

  const ides = $derived(editorsFor(project));
  // The project's editor — an explicit pick, else the auto-detected best fit.
  const bestFit = $derived(ides[0]);
  const hasAlternatives = $derived(ides.length > 1);

  // Re-profile on mount and whenever the active project changes. The store
  // coalesces this with any other surface's resolve, so the census runs once —
  // and its bookkeeping is non-reactive, so this reacts to `project` alone
  // (a reactive version once re-triggered here every completed fetch).
  $effect(() => {
    const workspace = project;
    refreshEditors(workspace);
  });

  function open(editor: Ide) {
    if (editor.terminal) {
      onterminaleditor(editor);
      return;
    }

    ide.open({
      command: editor.command,
      path: cwd
    });
  }
</script>

<!-- A newly-installed editor should show up without a restart: re-detect
     whenever the app becomes visible again (the user installed one in another
     app and switched back). Keyed off page *visibility*, not window focus — a
     Windows title-bar drag churns focus, and a focus-driven re-detection
     spawned processes mid-drag and lagged the drag, whereas visibility never
     changes while you drag a window that stays on screen. -->
<svelte:document
  onvisibilitychange={() => {
    if (!document.hidden) {
      refreshEditors(project);
    }
  }}
/>

{#if bestFit}
  <div class="ide menu-host">
    <button class="ide-open" onclick={() => open(bestFit)}>
      <span class="editor-glyph" data-brand={ideBrand(bestFit.id)}><Icon name={ideIcon(bestFit.id)} /></span>
      <span class="lbl">Open in {bestFit.label}</span>
    </button>
    <button
      style:anchor-name="--ide-anchor"
      class="ide-more menu-trigger"
      aria-label="Switch editor or reveal in file explorer"
      data-tooltip="Switch editor or reveal in file explorer"
      popovertarget="ide-menu"
    ><span class="caret">▾</span></button>

    <ul id="ide-menu" style:position-anchor="--ide-anchor" class="ide-list popover-menu" popover>
      <li class="hint">Open in editor</li>
      {#each ides as editor, index (editor.id)}
        <li>
          <button
            onclick={() => {
              // Picking from the list is an explicit choice: persisted, it
              // becomes the project's editor on every surface (this button and
              // the Change Feed's reveal) until picked otherwise.
              void chooseEditor({
                project,
                editorId: editor.id
              });
              open(editor);
            }}
            popovertarget="ide-menu"
            popovertargetaction="hide"
          >
            <span class="name">
              <span class="editor-glyph" data-brand={ideBrand(editor.id)}><Icon name={ideIcon(editor.id)} /></span>
              {editor.label}
            </span>
            {#if editor.chosen}
              <span class="best">your pick</span>
            {:else if index === 0 && hasAlternatives}
              <span class="best">best fit</span>
            {/if}
          </button>
        </li>
      {/each}
      <li class="sep" role="separator"></li>
      <li>
        <button onclick={() => void os.explorer(cwd)} popovertarget="ide-menu" popovertargetaction="hide">
          <span class="name"><Icon name="folder" /> Reveal in file explorer</span>
        </button>
      </li>
    </ul>
  </div>
{/if}

<style>
  /* The editor's brand colour (theme.css [data-brand]); a black brand
     (JetBrains) has no tint and follows the text colour. */
  .editor-glyph {
    display: inline-flex;
    flex: none;
    color: var(--brand-color, currentColor);
  }

  /* Split pill: a primary "open" action joined to a caret that opens the list.
     Both zones live in one surface-2 pill and light up independently on hover. */
  .ide {
    display: inline-flex;
    flex-shrink: 0;
    align-items: stretch;
    border-radius: 999px;
    background: var(--surface-2);
  }

  .ide-open {
    display: inline-flex;
    gap: 6px;
    align-items: center;
    padding-block: 7px;
    padding-inline: 13px 10px;
    border: none;
    border-radius: 999px;
    background: transparent;
    color: var(--on-surface);
    font: inherit;
    font-weight: 600;
    font-size: 13px;
    white-space: nowrap;
    cursor: pointer;
    transition: background 200ms var(--ease), color 200ms var(--ease);

    &:hover {
      background: var(--surface-3);
    }
  }

  /* When a caret follows, the open zone gives up its trailing round corners so
     the two read as one pill. */
  .ide:has(.ide-more) .ide-open {
    border-end-end-radius: 0;
    border-start-end-radius: 0;
  }

  .ide-more {
    position: relative;
    display: inline-flex;
    justify-content: center;
    align-items: center;
    padding-inline: 6px 9px;
    border: none;
    border-end-end-radius: 999px;
    border-start-end-radius: 999px;
    background: transparent;
    color: var(--on-surface-variant);
    cursor: pointer;
    transition: background 200ms var(--ease), color 200ms var(--ease);

    /* Hairline seam between the two zones. */
    &::before {
      content: "";
      position: absolute;
      inset-block: 6px;
      inset-inline-start: 0;
      inline-size: 1px;
      background: var(--outline);
    }

    &:hover {
      background: var(--surface-3);
      color: var(--on-surface);
    }

    .caret {
      font-size: 10px;
    }
  }

  /* Shell comes from the shared .popover-menu; only width and anchor side
     live here. */
  .ide-list {
    min-inline-size: 230px;
    position-area: bottom span-left;

    /* Uppercase section header at the top of the list. */
    .hint {
      padding-block: 6px 4px;
      padding-inline: 10px;
      color: var(--on-surface-variant);
      font-weight: 700;
      font-size: 10px;
      letter-spacing: 0.08em;
      text-transform: uppercase;
    }

    /* Hairline divider before the reveal action. */
    .sep {
      block-size: 1px;
      margin-block: 6px;
      margin-inline: 8px;
      background: var(--outline);
    }

    li button {
      display: flex;
      gap: 10px;
      justify-content: space-between;
      align-items: center;
      inline-size: 100%;
      padding: 8px 10px;
      border: none;
      border-radius: var(--radius-small);
      background: transparent;
      color: var(--on-surface);
      font: inherit;
      font-weight: 600;
      font-size: 13px;
      text-align: start;
      cursor: pointer;

      &:hover {
        background: var(--primary-container);
        color: var(--on-primary-container);
      }

      .name {
        display: flex;
        gap: 8px;
        align-items: center;
      }

      .best {
        color: var(--tertiary);
        font-weight: 700;
        font-size: 9px;
        letter-spacing: 0.06em;
        text-transform: uppercase;
      }
    }
  }
</style>
