<script lang="ts">
  import { tokenize } from "@/lib/highlight";
  import type { Token } from "@/lib/highlight";

  // Renders a snippet with lightweight syntax highlighting AND a color swatch
  // after every hex / rgb / hsl / var(--x) value. `vars` traces the file's own
  // token definitions so var(--x) swatches show the accurate color.
  //
  // This renders inside a <pre>, so whitespace is significant. Each token is a
  // SINGLE <span> (the swatch nests inside it), so every iteration is one node —
  // the only whitespace between tokens is the each-body boundary, which Svelte
  // trims. Two sibling nodes per iteration would instead leak a space per token.
  const { text, vars }: {
    text: string;
    vars?: Map<string, string>;
  } = $props();

  const tokens = $derived<Token[]>(tokenize(text, vars));
</script>

{#each tokens as token, index (index)}<span
  class:cmt={token.cls === "comment"}
  class:kw={token.cls === "keyword"}
  class:num={token.cls === "number"}
  class:str={token.cls === "string"}
>{token.text}{#if token.color}
  <span
    style:--sw={token.color}
    class="swatch"
    aria-hidden="true"
    title={token.color}
  ></span>
{/if}</span>{/each}

<style>
  .cmt {
    color: var(--syn-comment);
    font-style: italic;
  }

  .str {
    color: var(--syn-string);
  }

  .num {
    color: var(--syn-number);
  }

  .kw {
    color: var(--syn-keyword);
  }

  .swatch {
    display: inline-block;
    vertical-align: -0.12em;
    block-size: 0.85em;
    inline-size: 0.85em;
    margin-inline: 4px 1px;
    border-radius: 3px;
    background: var(--sw);

    /* An inset hairline so a swatch stays visible whether it's near-white on a
       light code surface or near-black on a dark one. */
    box-shadow: inset 0 0 0 1px color-mix(in sRGB, var(--on-surface) 32%, transparent);
  }
</style>
