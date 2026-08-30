<script lang="ts">
  import Icon from "@/lib/Icon.svelte";
  import { terminalSchemes } from "@/lib/stores/terminalSchemes.svelte";
  import { ANSI_COLOR_TOKENS } from "@/lib/terminal-theme";
  import type { TerminalScheme } from "@/lib/types";

  // One "which colour scheme paints the terminal" control: a trigger showing the
  // current pick with a preview strip, and a native popover listing every scheme
  // in the catalogue. `null` is the default — the terminal takes the app theme's
  // own palette — and it is an option in that same list rather than a special
  // case, so every row renders through one template. The catalogue comes from
  // the shared store, so this and the palette actually painted (lib/prefs)
  // resolve the same list.
  const { id, label, value, onpick }: {
    /** Popover id — must be unique among the pickers on screen. */
    id: string;
    /** Which app scheme this pick applies to ("Light"/"Dark"), or "" when one
     *  pick covers both. */
    label: string;
    value: string | null;
    onpick: (name: string | null) => void;
  } = $props();

  const FOLLOW_APP_THEME = "Follow the app theme";

  /** The slots the preview strip shows, named by the token that paints them, so
   *  the ANSI index comes from the one ANSI order (lib/terminal-theme) rather
   *  than from a literal here. */
  const PREVIEW_TOKENS = [
    "--terminal-red",
    "--terminal-yellow",
    "--terminal-green",
    "--terminal-cyan",
    "--terminal-blue",
    "--terminal-magenta"
  ] as const satisfies readonly (typeof ANSI_COLOR_TOKENS)[number][];
  const PREVIEW_SLOTS = PREVIEW_TOKENS.map(token => ANSI_COLOR_TOKENS.indexOf(token));

  /** The strip shown for "follow the app theme": the app's own accent tokens,
   *  which a terminal scheme never overwrites — so that row stays truthful about
   *  what it would paint whatever scheme is installed at the time. */
  const APP_THEME_COLORS = [
    "var(--primary)",
    "var(--warning)",
    "var(--tertiary)",
    "var(--context-ok)",
    "var(--on-surface-variant)",
    "var(--critical)"
  ];

  const schemes = $derived(terminalSchemes());
  /** The catalogue as the menu offers it: the default first, then every scheme. */
  const options = $derived<(TerminalScheme | null)[]>([null, ...schemes]);
  const chosen = $derived(schemes.find(scheme => scheme.name === value) ?? null);

  function optionName(scheme: TerminalScheme | null): string {
    return scheme?.name ?? FOLLOW_APP_THEME;
  }

  function pick(scheme: TerminalScheme | null): void {
    onpick(scheme?.name ?? null);
  }

  function previewBackground(scheme: TerminalScheme | null): string {
    return scheme?.background ?? "var(--surface-2)";
  }

  function previewColors(scheme: TerminalScheme | null): string[] {
    if (!scheme) {
      return APP_THEME_COLORS;
    }

    return PREVIEW_SLOTS.map(slot => scheme.ansi[slot]);
  }
</script>

{#snippet swatch(scheme: TerminalScheme | null)}
  <span style:background={previewBackground(scheme)} class="preview">
    {#each previewColors(scheme) as color, index (index)}
      <span style:background={color} class="stripe"></span>
    {/each}
  </span>
{/snippet}

<div class="picker menu-host">
  <span class="for-scheme">{label}</span>
  <button style:anchor-name="--{id}-anchor" class="trigger menu-trigger" popovertarget={id}>
    {@render swatch(chosen)}
    <span class="name">{optionName(chosen)}</span>
    <span class="caret">▾</span>
  </button>

  <ul {id} style:position-anchor="--{id}-anchor" class="scheme-list popover-menu scroll-fade" popover>
    {#each options as option (optionName(option))}
      <li>
        <button
          onclick={() => pick(option)}
          popovertarget={id}
          popovertargetaction="hide"
        >
          {@render swatch(option)}
          <span class="name">{optionName(option)}</span>
          {#if optionName(option) === optionName(chosen)}
            <Icon name="check" />
          {/if}
        </button>
      </li>
    {/each}
  </ul>
</div>

<style>
  .picker {
    display: flex;
    gap: 8px;
    align-items: center;
  }

  .for-scheme {
    flex: none;
    inline-size: 3rem;
    color: var(--on-surface-variant);
    font-weight: 600;
    font-size: 12px;

    /* One pick covering both schemes carries no label; the slot goes with it
       rather than leaving the trigger indented against nothing. */
    &:empty {
      display: none;
    }
  }

  .trigger {
    display: flex;
    flex: 1;
    gap: 8px;
    align-items: center;
    min-inline-size: 0;
    padding: 6px 10px;
    border: 1px solid var(--outline);
    border-radius: var(--radius-medium);
    background: var(--surface-1);
    color: var(--on-surface);
    font: inherit;
    font-size: 12px;
    cursor: pointer;
    transition: background 200ms var(--ease);

    &:hover {
      background: var(--surface-2);
    }
  }

  .preview {
    display: flex;
    flex: none;
    gap: 2px;
    justify-content: center;
    align-items: center;
    block-size: 20px;
    inline-size: 44px;
    padding: 3px 4px;
    border: 1px solid var(--outline);
    border-radius: 6px;
  }

  .stripe {
    block-size: 11px;
    inline-size: 4px;
    border-radius: 1px;
  }

  .name {
    flex: 1;
    overflow: hidden;
    text-align: start;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .caret {
    flex: none;
    color: var(--on-surface-variant);
  }

  .scheme-list {
    overflow-y: auto;
    max-block-size: 50vh;
    inline-size: max(16rem, anchor-size(inline));

    /* Below the trigger and right-aligned to it, so a menu wider than the side
       panel can never run off the window edge. */
    position-area: bottom span-left;

    /* The default sits apart from the catalogue it heads. */
    li:first-child {
      margin-block-end: 4px;
      padding-block-end: 4px;
      border-block-end: 1px solid var(--outline);
    }

    li button {
      display: flex;
      gap: 8px;
      align-items: center;
      inline-size: 100%;
      padding: 6px 8px;
      border: none;
      border-radius: var(--radius-small);
      background: transparent;
      color: var(--on-surface);
      font: inherit;
      font-size: 12px;
      cursor: pointer;

      &:hover {
        background: var(--surface-3);
      }
    }
  }
</style>
