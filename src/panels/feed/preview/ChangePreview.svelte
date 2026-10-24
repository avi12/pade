<script lang="ts">
  import type { DiffLine } from "@/lib/code/diff";
  import SandboxedPreview from "@/lib/code/SandboxedPreview.svelte";
  import { cardView, CardViewKind } from "@/lib/feed/preview/card-view";
  import type { FeedPreviews } from "@/lib/feed/preview/feed-previews";
  import Icon from "@/lib/Icon.svelte";
  import type { ChangeEvent, Ide } from "@/lib/types";
  import { baseName } from "@/lib/util/paths";
  import DiffPreview from "@/panels/feed/preview/DiffPreview.svelte";
  import MediaPreview from "@/panels/feed/preview/MediaPreview.svelte";

  // An open card's body: the picture, the clip, the rendered document or the
  // diff - or the state line saying why there is none yet (see card-view).
  const { event, project, editor, previews, lines, revealLine, isSplit }: {
    event: ChangeEvent;
    project: string;
    /** The ranked windowed editor a click on the diff opens the file in. */
    editor: Ide | undefined;
    previews: FeedPreviews;
    /** The card's cached diff lines (empty while there are none). */
    lines: DiffLine[];
    /** Where a reveal lands when no particular line was clicked. */
    revealLine: number | undefined;
    isSplit: boolean;
  } = $props();

  const view = $derived(
    cardView({
      path: event.path,
      pane: previews.paneOf(event.id),
      imageUrl: previews.imageCache.get(event.id),
      video: previews.videoCache.get(event.id),
      documentSource: previews.documentCache.get(event.id),
      lineCount: lines.length,
      isErrored: previews.failedIds.has(event.id)
    })
  );
</script>

{#if view.kind === CardViewKind.Diff}
  <DiffPreview {editor} {isSplit} {lines} path={event.path} {project} {revealLine} />
{:else if view.kind === CardViewKind.Message}
  <p class="state">{view.text}</p>
{:else if view.kind === CardViewKind.Loading}
  <p class="state loading"><span class="spinner"><Icon name="refresh" size={14} /></span>{view.text}</p>
{:else if view.kind === CardViewKind.Document}
  <SandboxedPreview document={view.source} label="Preview of {baseName(event.path)}" />
{:else}
  <MediaPreview name={baseName(event.path)} {view} />
{/if}

<style>
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
  }

  .spinner {
    display: inline-flex;
    color: var(--primary);
    animation: spin 900ms linear infinite;
  }
</style>
