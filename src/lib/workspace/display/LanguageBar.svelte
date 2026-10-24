<script lang="ts">
  import { observeProjectLanguages, projectLanguageShares } from "@/lib/stores/workspace/projectLanguages.svelte";
  import type { LanguageShare } from "@/lib/types";
  import { tooltip } from "@/lib/ui/truncation-tooltip";
  import {
    isEnteringAfterCount,
    isLanguageMix,
    languageShareText,
    segmentColors
  } from "@/lib/workspace/display/language-bar";
  import { untrack } from "svelte";

  // A project row's GitHub-style language bar: a slim bar along the row's foot,
  // one segment per language in proportion to its share of the source. Drawn
  // only for a project of two or more major languages; hovering a segment names
  // its language and share.
  const { path }: {
    path: string;
  } = $props();

  const shares = $derived(projectLanguageShares(path) ?? []);
  const colors = $derived(segmentColors(shares));

  // Segments grow in only when they land on screen - the list's first rows as
  // they are counted, a row a fast scroll reached before its count, a project
  // that gains a language - and never because a row already counted scrolls
  // into view. Cleared once the entrance is over or the row leaves the
  // screen, so a row coming back paints at rest.
  let isEntering = $state(false);
  let isOnScreen = false;
  let previousShares: LanguageShare[] | null | undefined;

  $effect.pre(() => {
    const current = projectLanguageShares(path);
    isEntering = isEnteringAfterCount({
      previous: previousShares,
      current,
      isEntering: untrack(() => isEntering),
      isOnScreen
    });
    previousShares = current;
  });

  function observeRow(element: Element) {
    return observeProjectLanguages({
      path,
      onScreenChange(isNowOnScreen) {
        isOnScreen = isNowOnScreen;

        if (!isNowOnScreen) {
          isEntering = false;
        }
      }
    })(element);
  }

  function settle() {
    isEntering = false;
  }

  // The shared top-layer bubble, not a `data-tooltip` pseudo: a project row
  // contains its own painting (content-visibility), which would clip a bubble
  // that opens below the row.
  function segmentTooltip(share: LanguageShare) {
    return tooltip(languageShareText(share));
  }
</script>

<span
  class="language-bar"
  {@attach observeRow}
  data-entering={isEntering || undefined}
  ontransitioncancel={settle}
  ontransitionend={settle}
>
  {#if isLanguageMix(shares)}
    {#each shares as share, i (share.language)}
      <span
        style:flex-grow={share.perMille}
        style:--language-color={colors[i]}
        {@attach segmentTooltip(share)}
      ></span>
    {/each}
  {/if}
</span>

<style>
  /* Each segment's colour comes from `language-bar` (one per language, swapped
     for its alternate when two in a bar would look alike). Each segment is
     taller than the colour it paints, so the hover target is easy to hit
     without a thick bar. It sits 3px up, inside the row so the hover and
     current-row outline never cross it, and always holds its place there
     (absolutely positioned, so it never moves the row): while entering (see
     `isEntering`) its segments grow in from the left, and a share that
     changes slides to its new width. */
  .language-bar {
    --bar-thickness: 4px;

    position: absolute;
    inset-inline: 8px;
    inset-block-end: 3px;
    display: flex;
    gap: 1px;
    align-items: flex-end;
    block-size: 7px;

    > span {
      flex-basis: 0;
      block-size: 100%;
      background:
        linear-gradient(
          to top,
          var(--language-color) 0 var(--bar-thickness),
          transparent var(--bar-thickness)
        );
      transition: flex-grow 400ms var(--ease), scale 400ms var(--ease);
      transform-origin: left;

      @media (prefers-reduced-motion: reduce) {
        transition: none;
      }

      &:hover {
        --bar-thickness: 100%;
      }
    }

    &[data-entering] > span {
      @starting-style {
        scale: 0 1;
      }
    }
  }
</style>
