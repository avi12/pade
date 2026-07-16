<script lang="ts">
  import { ide } from "@/lib/bridge";
  import Icon from "@/lib/Icon.svelte";
  import { ideBrand, ideIcon } from "@/lib/ideIcon";
  import type { Ide } from "@/lib/types";
  import { onMount } from "svelte";

  // Opens the active project in an external editor. `ide.suggest()` returns the
  // installed editors ranked for the detected project kind, so the best fit is
  // first — the split button's primary action opens it directly (auto-detected),
  // and the caret drops the full list to pick another. A console editor
  // (Neovim/Vim/Helix) can't run detached, so it's handed to the parent to open
  // in a PADE terminal tab instead of through the OS.
  const { onterminaleditor }: {
    onterminaleditor: (editor: Ide) => void;
  } = $props();

  let ides = $state<Ide[]>([]);
  // The auto-detected best fit for this project — the primary action's target.
  const bestFit = $derived(ides[0]);
  const hasAlternatives = $derived(ides.length > 1);

  // A newly-installed editor should show up without a restart: re-detect once at
  // mount and whenever the app becomes visible again (the user installed one in
  // another app and switched back). We key off page *visibility*, not window focus —
  // a Windows title-bar drag churns focus and any focus-driven `ide.suggest()`
  // spawned editor-detection processes mid-drag and lagged the drag, whereas
  // visibility never changes while you drag a window that stays on screen.
  async function detect() {
    ides = await ide.suggest();
  }
  onMount(detect);

  function open(editor: Ide) {
    if (editor.terminal) {
      onterminaleditor(editor);
      return;
    }

    ide.open({ command: editor.command });
  }
</script>

<svelte:document
  onvisibilitychange={() => {
    if (!document.hidden) void detect();
  }}
/>

{#if bestFit}
  <span class="ide">
    <button class="ide-open" onclick={() => open(bestFit)}>
      <span class="editor-glyph" data-brand={ideBrand(bestFit.id)}><Icon name={ideIcon(bestFit.id)} /></span>
      <span class="lbl">Open in {bestFit.label}</span>
    </button>
    {#if hasAlternatives}
      <button
        style:anchor-name="--ide-anchor"
        class="ide-more"
        aria-label="Choose a different editor"
        data-tooltip="Choose a different editor"
        popovertarget="ide-menu"
      ><span class="caret">▾</span></button>
    {/if}
  </span>

  {#if hasAlternatives}
    <ul id="ide-menu" style:position-anchor="--ide-anchor" class="ide-list popover-menu" popover>
      {#each ides as editor, index (editor.id)}
        <li>
          <button
            onclick={() => open(editor)}
            popovertarget="ide-menu"
            popovertargetaction="hide"
          >
            <span class="name">
              <span class="editor-glyph" data-brand={ideBrand(editor.id)}><Icon name={ideIcon(editor.id)} /></span>
              {editor.label}
            </span>
            {#if index === 0}
              <span class="best">best fit</span>
            {/if}
          </button>
        </li>
      {/each}
    </ul>
  {/if}
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
    padding: 7px 13px;
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
    padding-inline: 8px 11px;
    border: none;
    border-end-end-radius: 999px;
    border-start-end-radius: 999px;
    background: transparent;
    color: var(--on-surface);
    cursor: pointer;
    transition: background 200ms var(--ease);

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
    }

    .caret {
      font-size: 10px;
      opacity: 70%;
    }
  }

  /* Shell comes from the shared .popover-menu; only width and anchor side
     live here. */
  .ide-list {
    min-inline-size: 200px;
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
