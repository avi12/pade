<script lang="ts">
  import Icon from "@/lib/Icon.svelte";
  import { ideBrand, ideIcon } from "@/lib/ide/ide-icon";
  import { Command, sendMessage } from "@/lib/ipc/messaging";
  import { chooseEditor, editorsFor, refreshEditors } from "@/lib/stores/workspace/editors.svelte";
  import type { Ide } from "@/lib/types";

  // Opens the active project in an external editor. The ranked list comes from
  // the shared editors store (SSOT - the same list the Change Feed's reveal
  // reads), whose backend `ide_suggest` puts the project's editor first: an
  // explicit pick from this menu, else the best auto-detected fit. The split
  // button's primary action opens that editor directly, and the caret always
  // drops the full list, ending with File Explorer. A console editor
  // (Neovim/Vim/Helix) can't run detached, so it's handed to the parent to open
  // in a PADE terminal tab instead of through the OS.
  const { onterminaleditor, project, cwd }: {
    onterminaleditor: (editor: Ide) => void;
    /** The open project - the key the shared editor ranking is resolved under,
     *  shared with the Change Feed so the two surfaces can't drift. */
    project: string;
    /** The directory the launcher opens: the active session's worktree when one
     *  is focused, else the project itself. Also the target of "Reveal in file
     *  explorer", so the selector always has that final action. */
    cwd: string;
  } = $props();

  const MENU_ID = "ide-menu";
  const MENU_ANCHOR = "--ide-anchor";

  const ides = $derived(editorsFor(project));
  // The project's editor - an explicit pick, else the auto-detected best fit.
  const bestFit = $derived(ides[0]);
  const hasAlternatives = $derived(ides.length > 1);

  // Re-profile on mount and whenever the active project changes. The store
  // coalesces this with any other surface's resolve, so the census runs once -
  // and its bookkeeping is non-reactive, so this reacts to `project` alone
  // (a reactive version once re-triggered here every completed fetch).
  $effect(() => {
    const workspace = project;
    refreshEditors(workspace);
  });

  async function open(editor: Ide) {
    if (editor.terminal) {
      onterminaleditor(editor);
      return;
    }

    await sendMessage(Command.ideOpen, {
      command: editor.command,
      path: cwd
    });
  }
</script>

<!-- A newly-installed editor should show up without a restart: re-detect
     whenever the app becomes visible again (the user installed one in another
     app and switched back). Keyed off page *visibility*, not window focus - a
     Windows title-bar drag churns focus, and a focus-driven re-detection
     spawned processes mid-drag and lagged the drag, whereas visibility never
     changes while you drag a window that stays on screen. -->
<svelte:document
  onvisibilitychange={async () => {
    if (!document.hidden) {
      await refreshEditors(project);
    }
  }}
/>

{#if bestFit}
  <div class="ide menu-host">
    <button class="ide-open" onclick={async () => await open(bestFit)}>
      <span class="editor-glyph" data-brand={ideBrand(bestFit.id)}><Icon name={ideIcon(bestFit.id)} /></span>
      <span class="label">Open in {bestFit.label}</span>
    </button>
    <button
      style:anchor-name={MENU_ANCHOR}
      class="ide-more menu-trigger"
      aria-label="Switch editor or reveal in file explorer"
      data-tooltip="Switch editor or reveal in file explorer"
      popovertarget={MENU_ID}
    ><span class="caret">▾</span></button>

    <ul id={MENU_ID} style:position-anchor={MENU_ANCHOR} class="ide-list popover-menu" popover>
      <li class="menu-separator">Open in editor</li>
      {#each ides as editor, i (editor.id)}
        <li>
          <button
            onclick={async () => {
              // Picking from the list is an explicit choice: persisted, it
              // becomes the project's editor on every surface (this button and
              // the Change Feed's reveal) until picked otherwise. The launch
              // doesn't wait on the save.
              await Promise.all([
                chooseEditor({
                  project,
                  editorId: editor.id
                }),
                open(editor)
              ]);
            }}
            popovertarget={MENU_ID}
            popovertargetaction="hide"
          >
            <span class="name">
              <span class="editor-glyph" data-brand={ideBrand(editor.id)}><Icon name={ideIcon(editor.id)} /></span>
              {editor.label}
            </span>
            {#if editor.chosen}
              <span class="best">your pick</span>
            {:else if i === 0 && hasAlternatives}
              <span class="best">best fit</span>
            {/if}
          </button>
        </li>
      {/each}
      <li class="menu-divider" role="separator"></li>
      <li>
        <button
          onclick={async () => await sendMessage(Command.openInExplorer, { path: cwd })}
          popovertarget={MENU_ID}
          popovertargetaction="hide"
        >
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
    border-radius: var(--radius-full);
    background: var(--surface-2);
  }

  .ide-open {
    display: inline-flex;
    gap: 6px;
    align-items: center;
    padding-block: 7px;
    padding-inline: 13px 10px;
    border: none;
    border-radius: var(--radius-full);
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
    border-end-end-radius: var(--radius-full);
    border-start-end-radius: var(--radius-full);
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
