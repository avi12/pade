<!--
  Find-in-terminal — the Ctrl+F bar that floats over a pane's output.

  What it searches is xterm's ACTIVE buffer: a shell's scrollback when the
  session has one, and just the visible frame while an agent holds the alternate
  screen (a fullscreen TUI keeps its transcript in its own process, not in the
  terminal's document — see docs/terminal-rendering.md). The searching itself is
  xterm's own search addon, loaded on first open so a session that never searches
  never pays for it.

  The bar owns its open state: the pane asks for it with `reveal()` and is told
  when it closes, so the keyboard can go back to the agent.
-->
<script lang="ts">
  import { errorMessage } from "@/lib/errors";
  import Icon from "@/lib/Icon.svelte";
  import { showToast } from "@/lib/stores/toast.svelte";
  import {
    defaultFindOptions,
    FIND_OPTION_TOGGLES,
    FindDirection,
    findResultLabel,
    isFindShortcut,
    isSearchablePattern,
    matchFindOptionChord
  } from "@/lib/terminal-find";
  import type { FindOption } from "@/lib/terminal-find";
  import { rootTokenReader, xtermSearchDecorations } from "@/lib/terminal-theme";
  import { parseInput, TerminalFindTerm } from "@/lib/validate";
  import type { SearchAddon } from "@xterm/addon-search";
  import type { Terminal } from "@xterm/xterm";
  import { onDestroy, tick, untrack } from "svelte";

  const { terminal, palette, onclose }: {
    /** Undefined until the pane's terminal has been built and attached. */
    terminal: Terminal | undefined;
    /** Everything that decides the terminal's colours, as one value. A change
        re-runs the live search so the highlights are re-drawn in the new
        scheme — the addon paints with the options it was last given. */
    palette: string;
    /** The bar closed — the pane takes the keyboard back. */
    onclose: () => void;
  } = $props();

  let open = $state(false);
  let term = $state("");
  // How the term is matched. Kept for the life of the pane, so a search you
  // refine (case on, regex on) survives closing and re-opening the bar.
  const matching = $state(defaultFindOptions());
  let resultIndex = $state(-1);
  let resultCount = $state(0);
  let input = $state<HTMLInputElement>();
  let searchAddon: SearchAddon | undefined;

  const searchable = $derived(
    isSearchablePattern({
      term,
      regex: matching.regex
    })
  );

  const label = $derived(
    findResultLabel({
      term,
      resultIndex,
      resultCount,
      searchable
    })
  );

  async function loadedAddon(): Promise<SearchAddon | undefined> {
    if (searchAddon || !terminal) {
      return searchAddon;
    }

    try {
      const { SearchAddon } = await import("@xterm/addon-search");
      const loaded = new SearchAddon();
      terminal.loadAddon(loaded);
      loaded.onDidChangeResults(results => {
        resultIndex = results.resultIndex;
        resultCount = results.resultCount;
      });
      searchAddon = loaded;
      return loaded;
    } catch (error) {
      showToast(
        errorMessage({
          error,
          fallback: "Terminal search is unavailable."
        })
      );
      return undefined;
    }
  }

  async function runSearch({ direction, incremental = false }: {
    direction: FindDirection;
    incremental?: boolean;
  }): Promise<void> {
    const addon = await loadedAddon();
    if (!addon) {
      return;
    }

    // The box is a free-text field like any other: what reaches the search
    // engine is what the schema passed, never the raw input value. A regex the
    // user is still halfway through typing is held back the same way — it would
    // throw inside the engine on the keystroke that reached it.
    const query = parseInput({
      schema: TerminalFindTerm,
      raw: term
    });
    if (query === null || !searchable) {
      addon.clearDecorations();
      resultIndex = -1;
      resultCount = 0;
      return;
    }

    const options = {
      ...matching,
      decorations: xtermSearchDecorations({ readToken: rootTokenReader() }),
      incremental
    };
    if (direction === FindDirection.Previous) {
      addon.findPrevious(query, options);
      return;
    }

    addon.findNext(query, options);
  }

  // Typing keeps the caret on the match it is already sitting in (`incremental`)
  // so the view doesn't jump a screen per keystroke; Enter is what advances.
  async function searchAsTyped(): Promise<void> {
    await runSearch({
      direction: FindDirection.Next,
      incremental: true
    });
  }

  async function step(direction: FindDirection): Promise<void> {
    await runSearch({ direction });
  }

  // Re-run the current search from scratch. The addon's highlighted set has to be
  // cleared first, because only a changed TERM invalidates it: `findNext` stores
  // the options it was handed and then asks whether they changed by comparing
  // that same object against itself, so a flipped rule (and the recoloured
  // decorations a palette flip wants) would otherwise leave the old highlights
  // and the old count standing while only the active match moved.
  async function researchFromScratch(): Promise<void> {
    searchAddon?.clearDecorations();
    await searchAsTyped();
  }

  // Flipping a toggle re-runs the search under the new rule, so the highlights
  // and the count answer the question the bar is now asking.
  async function toggleMatching(option: FindOption): Promise<void> {
    matching[option] = !matching[option];
    await researchFromScratch();
  }

  /** Open the bar (or bring the caret back to it) with the previous term
   *  selected, so the next keystroke replaces it and Enter repeats it. */
  export async function reveal(): Promise<void> {
    open = true;
    await tick();
    input?.focus();
    input?.select();
    await searchAsTyped();
  }

  function close(): void {
    open = false;
    searchAddon?.clearDecorations();
    resultIndex = -1;
    resultCount = 0;
    onclose();
  }

  // Re-run the search when the scheme or terminal palette changes under it, so
  // open highlights are repainted in the colours the rest of the app just took.
  $effect(() => {
    const painted = palette;
    if (!open || !painted) {
      return;
    }

    untrack(researchFromScratch);
  });

  onDestroy(() => {
    searchAddon?.dispose();
    searchAddon = undefined;
  });
</script>

{#if open}
  <search class="find-bar">
    <span class="lead" aria-hidden="true"><Icon name="search" size={14} /></span>
    <input
      bind:this={input}
      aria-label="Find in terminal"
      autocapitalize="off"
      autocomplete="off"
      oninput={searchAsTyped}
      onkeydown={async e => {
        if (e.key === "Escape") {
          e.preventDefault();
          close();
          return;
        }

        if (isFindShortcut(e)) {
          e.preventDefault();
          input?.select();
          return;
        }

        const toggled = matchFindOptionChord(e);
        if (toggled) {
          e.preventDefault();
          await toggleMatching(toggled);
          return;
        }

        if (e.key !== "Enter") {
          return;
        }

        e.preventDefault();
        await step(e.shiftKey ? FindDirection.Previous : FindDirection.Next);
      }}
      placeholder="Find in terminal"
      spellcheck="false"
      type="text"
      bind:value={term}
    />
    <span class="toggles">
      {#each FIND_OPTION_TOGGLES as toggle (toggle.option)}
        <button
          class="toggle"
          aria-label={toggle.label}
          aria-pressed={matching[toggle.option]}
          data-tooltip={`${toggle.label} · Alt+${toggle.chordKey.toUpperCase()}`}
          onclick={async () => {
            await toggleMatching(toggle.option);
            input?.focus();
          }}
          type="button"
        >
          <Icon name={toggle.icon} size={14} />
        </button>
      {/each}
    </span>
    <output class="count" class:unsearchable={!searchable}>{label}</output>
    <button
      class="step"
      aria-label="Previous match"
      data-tooltip="Previous match · Shift+Enter"
      disabled={resultCount === 0}
      onclick={async () => {
        await step(FindDirection.Previous);
      }}
      type="button"
    >
      <Icon name="chevronUp" size={14} />
    </button>
    <button
      class="step"
      aria-label="Next match"
      data-tooltip="Next match · Enter"
      disabled={resultCount === 0}
      onclick={async () => {
        await step(FindDirection.Next);
      }}
      type="button"
    >
      <Icon name="chevronDown" size={14} />
    </button>
    <button
      class="dismiss"
      aria-label="Close find"
      data-tooltip="Close · Esc"
      onclick={close}
      type="button"
    >
      <Icon name="close" size={14} />
    </button>
  </search>
{/if}

<style>
  /* Floats over the output rather than sitting above it: the pane's measured
     viewport decides the PTY's rows and columns, so a bar that took layout space
     would resize the agent's grid every time it opened. */
  .find-bar {
    position: absolute;
    inset-block-start: 4px;
    inset-inline-end: 4px;
    z-index: 2;
    display: flex;
    gap: 4px;
    align-items: center;
    padding-block: 5px;
    padding-inline: 9px 5px;
    border: 1px solid var(--outline);
    border-radius: var(--radius-medium);
    background: var(--surface-1);
    box-shadow: 0 6px 18px var(--shadow-color);
    animation: find-bar-in 140ms var(--ease);

    .lead {
      display: inline-flex;
      flex: none;
      color: var(--on-surface-variant);
    }

    input {
      inline-size: 20ch;
      padding: 2px 4px;
      border: none;
      background: transparent;
      color: var(--on-surface);
      font-family: var(--font-monospace);
      font-size: 12px;

      &:focus-visible {
        outline: none;
      }

      &::placeholder {
        color: var(--on-surface-variant);
      }
    }

    /* The count changes on every keystroke — tabular figures keep it from
       shuffling the buttons sideways as the digits change. */
    .count {
      flex: none;
      min-inline-size: 8ch;
      color: var(--on-surface-variant);
      font-family: var(--font-monospace);
      font-size: 11px;
      font-variant-numeric: tabular-nums;
      text-align: end;
    }

    .toggles {
      display: inline-flex;
      flex: none;
      gap: 2px;
      align-items: center;
    }

    /* A pressed toggle is a filled plate, not a tinted glyph: the bar is small
       and sits over live output, so "this rule is on" has to survive being read
       at a glance against whatever the agent happens to be painting behind it. */
    .toggle[aria-pressed="true"] {
      background: var(--primary-container);
      color: var(--on-primary-container);
    }

    button {
      display: inline-flex;
      flex: none;
      justify-content: center;
      align-items: center;
      block-size: 22px;
      inline-size: 22px;
      border: none;
      border-radius: var(--radius-full);
      background: transparent;
      color: var(--on-surface-variant);
      cursor: pointer;
      transition: color 150ms var(--ease), background 150ms var(--ease);

      &:hover:not(:disabled) {
        background: var(--surface-3);
        color: var(--on-surface);
      }

      &:disabled {
        opacity: 40%;
        cursor: default;
      }
    }

    /* A half-typed pattern is not a result — say so in the warning colour rather
       than let "No matches" imply the terminal was searched and came up empty. */
    .count.unsearchable {
      color: var(--warning);
    }

    .dismiss:hover {
      background: var(--critical-wash);
      color: var(--critical);
    }
  }

  @keyframes find-bar-in {
    from {
      opacity: 0%;
      translate: 0 -6px;
    }
  }
</style>
