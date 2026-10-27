<script lang="ts">
  import SandboxedPreview from "@/lib/code/SandboxedPreview.svelte";
  import { CardViewKind, isMediaView } from "@/lib/feed/preview/card-view";
  import type { CardView } from "@/lib/feed/preview/card-view";
  import MediaPreview from "@/lib/feed/preview/MediaPreview.svelte";
  import Icon from "@/lib/Icon.svelte";
  import type { Snippet } from "svelte";

  // A preview as `card-view` decides it: the picture, the clip, the rendered
  // document, the caller's own diff - or the state line saying why there is
  // none yet. Shared by a Change Feed card and the commit modal's file pane.
  const { view, name, isFilling = false, diff }: {
    view: CardView;
    /** The file's name, for the media's alt text and the frame's label. */
    name: string;
    /** Media fills the parent's height instead of a card-sized strip. */
    isFilling?: boolean;
    /** The diff, drawn by the caller (each view wires its own interactions). */
    diff: Snippet;
  } = $props();
</script>

{#if view.kind === CardViewKind.Diff}
  {@render diff()}
{:else if view.kind === CardViewKind.Document}
  <SandboxedPreview document={view.source} label="Preview of {name}" />
{:else if isMediaView(view)}
  <MediaPreview {name} {isFilling} {view} />
{:else}
  <p class="state" class:loading={view.kind === CardViewKind.Loading}>
    <span class="spinner"><Icon name="refresh" size={14} /></span>{view.text}
  </p>
{/if}

<style>
  .spinner {
    display: inline-flex;
    color: var(--primary);
    animation: spin 900ms linear infinite;
  }

  .state {
    margin: 0;
    padding: 14px 12px;
    background: var(--code-background);
    color: var(--on-surface-variant);
    font-size: 0.75rem;

    &.loading {
      display: flex;
      gap: 8px;
      align-items: center;
    }

    &:not(.loading) .spinner {
      display: none;
    }
  }
</style>
