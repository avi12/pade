<script lang="ts">
  import { openLink } from "@/lib/dashboard/open-link";
  import { InlineKind, inlineMarkdown, withPullRequestLinks } from "@/lib/dashboard/summary-markdown";
  import type { PullRequest } from "@/lib/types";

  // An agent summary's inline Markdown as real elements - bold, emphasis, code
  // and links - never as HTML. A link opens in the browser, not in this window;
  // a mention of the project's open pull request becomes a link to it.
  const { text, pullRequest }: {
    text: string;
    pullRequest: PullRequest | null;
  } = $props();

  /** The element each formatted piece renders as. */
  const INLINE_TAGS = {
    [InlineKind.Strong]: "strong",
    [InlineKind.Emphasis]: "em",
    [InlineKind.Code]: "code",
    [InlineKind.Link]: "a",
    [InlineKind.Text]: "span"
  } as const;

  const pieces = $derived(
    inlineMarkdown(
      withPullRequestLinks({
        text,
        pullRequest
      })
    )
  );
</script>

{#each pieces as piece, i (i)}
  {#if piece.url}
    <a
      href={piece.url}
      onclick={async e => {
        e.preventDefault();
        await openLink(e.currentTarget.href);
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
