<script lang="ts">
  import type { DiffLine } from "@/lib/code/diff";
  import { cardView } from "@/lib/feed/preview/card-view";
  import type { FeedPreviews } from "@/lib/feed/preview/feed-previews";
  import PreviewBody from "@/lib/feed/preview/PreviewBody.svelte";
  import type { ChangeEvent, Ide } from "@/lib/types";
  import { baseName } from "@/lib/util/paths";
  import DiffPreview from "@/panels/feed/preview/DiffPreview.svelte";

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

<PreviewBody name={baseName(event.path)} {view}>
  {#snippet diff()}
    <DiffPreview {editor} {isSplit} {lines} path={event.path} {project} {revealLine} />
  {/snippet}
</PreviewBody>
