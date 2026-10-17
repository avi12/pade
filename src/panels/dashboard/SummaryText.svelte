<script lang="ts">
  import { InlineKind, inlineMarkdown } from "@/lib/dashboard/summary-markdown";
  import { Command, sendMessage } from "@/lib/ipc/messaging";

  // An agent summary's inline Markdown as real elements - bold, emphasis, code
  // and links - never as HTML. A link opens in the browser, not in this window.
  const { text }: {
    text: string;
  } = $props();

  /** The element each formatted piece renders as. */
  const INLINE_TAGS = {
    [InlineKind.Strong]: "strong",
    [InlineKind.Emphasis]: "em",
    [InlineKind.Code]: "code",
    [InlineKind.Link]: "a",
    [InlineKind.Text]: "span"
  } as const;

  const pieces = $derived(inlineMarkdown(text));
</script>

{#each pieces as piece, i (i)}
  {#if piece.url}
    <a
      href={piece.url}
      onclick={async e => {
        e.preventDefault();
        try {
          await sendMessage(Command.openUrl, { url: e.currentTarget.href });
        } catch (error) {
          console.warn("pade: could not open the summary's link", error);
        }
      }}
    >{piece.text}</a>
  {:else if piece.kind === InlineKind.Text}
    {piece.text}
  {:else}
    <svelte:element this={INLINE_TAGS[piece.kind]}>{piece.text}</svelte:element>
  {/if}
{/each}

<style>
  code {
    padding: 0 3px;
    border-radius: var(--radius-small);
    background: var(--surface-3);
    font-family: var(--font-monospace);
    font-size: 0.92em;
  }

  /* Above the card's stretched open-window button, so a link opens itself. */
  a {
    position: relative;
    z-index: 1;
    color: var(--primary);
    text-decoration: underline;
    text-underline-offset: 2px;
  }
</style>
