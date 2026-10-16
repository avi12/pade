<script lang="ts">
  import { observeProjectLanguages, projectLanguageShares } from "@/lib/stores/workspace/projectLanguages.svelte";
  import { isLanguageMix, languageShareText, segmentColors } from "@/lib/workspace/language-bar";

  // A project row's GitHub-style language bar: a slim bar along the row's foot,
  // one segment per language in proportion to its share of the source. Drawn
  // only for a project of two or more major languages; hovering a segment names
  // its language and share.
  const { path }: {
    path: string;
  } = $props();

  const shares = $derived(projectLanguageShares(path) ?? []);
  const colors = $derived(segmentColors(shares));
</script>

<span class="language-bar" {@attach observeProjectLanguages({ path })}>
  {#if isLanguageMix(shares)}
    {#each shares as share, i (share.language)}
      <span
        style:flex-grow={share.perMille}
        style:--language-color={colors[i]}
        data-tooltip={languageShareText(share)}
      ></span>
    {/each}
  {/if}
</span>

<style>
  /* Each segment's colour comes from `language-bar` (one per language, swapped
     for its alternate when two in a bar would look alike). Each segment is
     taller than the colour it paints, so the hover target is easy to hit
     without a thick bar. */
  .language-bar {
    --bar-thickness: 4px;

    position: absolute;
    inset-inline: 8px;
    inset-block-end: 0;
    display: flex;
    gap: 1px;
    align-items: flex-end;
    block-size: 8px;

    > span {
      flex-basis: 0;
      block-size: 100%;
      background:
        linear-gradient(
          to top,
          var(--language-color) 0 var(--bar-thickness),
          transparent var(--bar-thickness)
        );

      &:hover {
        --bar-thickness: 100%;
      }
    }
  }
</style>
