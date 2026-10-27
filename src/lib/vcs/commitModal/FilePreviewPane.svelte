<script lang="ts">
  import DiffView from "@/lib/code/DiffView.svelte";
  import { cardView, CardViewKind } from "@/lib/feed/preview/card-view";
  import type { CardView } from "@/lib/feed/preview/card-view";
  import { PreviewPane } from "@/lib/feed/preview/feed-previews";
  import type { FeedPreviews, PreviewCard } from "@/lib/feed/preview/feed-previews";
  import { isMediaPath, isRenderablePath } from "@/lib/feed/preview/preview";
  import PreviewBody from "@/lib/feed/preview/PreviewBody.svelte";
  import ViewToggles from "@/lib/feed/preview/ViewToggles.svelte";
  import Icon from "@/lib/Icon.svelte";
  import { DiffStyle } from "@/lib/types";
  import { baseName } from "@/lib/util/paths";
  import type { z } from "zod";

  // The commit modal's right pane: path bar + the selected file as the commit
  // left it - the picture, the clip, the rendered document or the diff, chosen
  // by the same `card-view` the Change Feed uses - with the loading / failed /
  // large-file states. The modal owns the fetches (`previews`); this only draws.
  let { card, isBinary, previews, diffMode = $bindable(), commitUrl, onopenremote }: {
    /** The selected file, cached under its path. */
    card: PreviewCard;
    /** Git reports the file as binary, so it has no textual diff. */
    isBinary: boolean;
    previews: FeedPreviews;
    /** The modal-wide Unified|Split choice, kept across files. */
    diffMode: z.infer<typeof DiffStyle>;
    /** Browsable remote commit URL, or null when there's no remote. */
    commitUrl: string | null;
    onopenremote: () => void;
  } = $props();

  const name = $derived(baseName(card.path));
  const lines = $derived(previews.diffCache.get(card.id) ?? []);
  const pane = $derived(previews.paneOf(card.id));
  const isErrored = $derived(previews.failedIds.has(card.id));
  const isMedia = $derived(isMediaPath(card.path));
  const canPreview = $derived(isRenderablePath(card.path));
  const isCodeShown = $derived(!isMedia && pane === PreviewPane.Code);
  const isFetching = $derived(isCodeShown && !previews.isCardPreviewLoaded(card));
  const canSwitchDiffMode = $derived(isCodeShown && lines.length > 0);
  const hasViewChoices = $derived(canPreview || canSwitchDiffMode);
  // Binary / very large: git emits no textual diff, so we show the note. A load
  // error is a distinct state and must not read as a big file.
  const isBigFile = $derived(
    isCodeShown && !isFetching && !isErrored && (isBinary || lines.length === 0)
  );
  // The feed loads a card before opening it; here the pane is already open, so
  // a diff still on its way reads as loading rather than as no preview.
  const view = $derived.by(() => {
    if (isFetching) {
      return {
        kind: CardViewKind.Loading,
        text: "Loading the diff"
      } satisfies CardView;
    }

    return cardView({
      path: card.path,
      pane,
      imageUrl: previews.imageCache.get(card.id),
      video: previews.videoCache.get(card.id),
      documentSource: previews.documentCache.get(card.id),
      lineCount: lines.length,
      isErrored
    });
  });
  const isFilled = $derived(
    view.kind === CardViewKind.Image || view.kind === CardViewKind.Video || view.kind === CardViewKind.Document
  );
</script>

<div class="pane">
  <div class="pane-bar">
    <span class="pane-path">{name}</span>
    {#if isBigFile}
      <span class="big-note">large file · diff omitted</span>
    {/if}
    {#if hasViewChoices}
      <ViewToggles {canPreview} {canSwitchDiffMode} {card} {previews} bind:diffMode />
    {/if}
  </div>
  <div
    id="commit-diff"
    class="diff"
    class:filled={isFilled}
    aria-label="Preview of {name}"
    role="tabpanel"
    tabindex="0"
  >
    {#if isBigFile}
      <div class="omit">
        <span class="omit-text">
          This file is too large to render inline. The rest of the diff is available on the remote.
        </span>
        <button class="omit-button" disabled={!commitUrl} onclick={onopenremote}>
          <Icon name="external" size={14} /> View full diff
        </button>
      </div>
    {:else}
      <PreviewBody {name} isFilling {view}>
        {#snippet diff()}
          <DiffView diffLines={lines} split={diffMode === DiffStyle.enum.split} />
        {/snippet}
      </PreviewBody>
    {/if}
  </div>
</div>

<style>
  .pane {
    display: flex;
    flex-direction: column;
    min-block-size: 0;
    background: var(--code-background);
  }

  .pane-bar {
    display: flex;
    flex: none;
    gap: 8px;
    align-items: center;
    padding-block: 9px;
    padding-inline: 14px;
    border-block-end: 1px solid var(--outline);
    background: var(--surface-2);

    .pane-path {
      flex: 1;
      overflow: hidden;
      min-inline-size: 0;
      color: var(--on-surface);
      font-family: var(--font-monospace);
      font-weight: 600;
      font-size: 0.75rem;
      text-overflow: ellipsis;
      white-space: nowrap;
    }

    .big-note {
      flex: none;
      padding-block: 3px;
      padding-inline: 9px;
      border-radius: var(--radius-full);
      background: var(--warning-wash);
      color: var(--warning);
      font-weight: 700;
      font-size: 0.625rem;
      letter-spacing: 0.04em;
      text-transform: uppercase;
      white-space: nowrap;
    }
  }

  /* Scroll container around the shared DiffView; the wider gutter matches the
     pane bar's 14px inline padding. A picture, clip or rendered document fills
     the pane edge to edge instead (its one grid cell sized to the pane, so the
     media and the iframe can take its full height). */
  .diff {
    --diff-line-padding: 14px;

    flex: 1;
    overflow: auto;
    min-block-size: 0;
    padding-block: 10px;

    &.filled {
      --preview-height: 100%;

      display: grid;
      grid-template: minmax(0, 1fr) / minmax(0, 1fr);
      overflow: hidden;
      padding-block: 0;
    }
  }

  .omit {
    display: flex;
    gap: 12px;
    align-items: center;
    margin-block: 12px 4px;
    margin-inline: 14px;
    padding: 14px;
    border: 1px dashed var(--outline);
    border-radius: var(--radius-medium);
    background: var(--surface-1);

    .omit-text {
      flex: 1;
      color: var(--on-surface-variant);
      font-size: 0.75rem;
      line-height: 1.5;
    }

    .omit-button {
      display: inline-flex;
      flex: none;
      gap: 7px;
      align-items: center;
      padding-block: 8px;
      padding-inline: 15px;
      border: none;
      border-radius: var(--radius-full);
      background: var(--primary);
      color: var(--on-primary);
      font: inherit;
      font-weight: 700;
      font-size: 0.75rem;
      cursor: pointer;
      transition: filter 120ms var(--ease);

      &:hover:not(:disabled) {
        filter: brightness(1.06);
      }

      &:disabled {
        opacity: 55%;
        cursor: default;
      }
    }
  }
</style>
