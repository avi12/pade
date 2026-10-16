<script lang="ts">
  import { observeProjectLanguages, projectLanguageShares } from "@/lib/stores/workspace/projectLanguages.svelte";
  import { isLanguageMix, languageShareText } from "@/lib/workspace/language-bar";

  // A project row's GitHub-style language bar: a slim bar along the row's foot,
  // one segment per language in proportion to its share of the source. Drawn
  // only for a project of two or more major languages; hovering a segment names
  // its language and share.
  const { path }: {
    path: string;
  } = $props();

  const shares = $derived(projectLanguageShares(path) ?? []);
</script>

<span class="language-bar" {@attach observeProjectLanguages({ path })}>
  {#if isLanguageMix(shares)}
    {#each shares as share (share.kind)}
      <span style:flex-grow={share.perMille} data-language={share.kind} data-tooltip={languageShareText(share)}></span>
    {/each}
  {/if}
</span>

<style>
  /* One clearly different hue per language, so no two segments of a bar can be
     mistaken for each other - each taken from its language's own logo where that
     keeps it distinct (TypeScript blue, Python yellow, Java orange, C# purple).
     An umbrella kind wears its most common member's colour. Each segment is
     taller than the colour it paints, so the hover target is easy to hit without
     a thick bar. */
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
      --language-color: var(--outline);

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

    [data-language="web"] {
      --language-color: #3178c6;
    }

    [data-language="python"] {
      --language-color: #ffd43b;
    }

    [data-language="java"] {
      --language-color: #e76f00;
    }

    [data-language="go"] {
      --language-color: #00add8;
    }

    [data-language="rust"] {
      --language-color: #dea584;
    }

    [data-language="android"] {
      --language-color: #3ddc84;
    }

    [data-language="cpp"] {
      --language-color: #f34b7d;
    }

    [data-language="dotnet"] {
      --language-color: #9b4f96;
    }

    [data-language="php"] {
      --language-color: #8892bf;
    }

    [data-language="ruby"] {
      --language-color: #e0443b;
    }
  }
</style>
