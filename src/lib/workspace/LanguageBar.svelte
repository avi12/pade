<script lang="ts">
  import { observeProjectLanguages, projectLanguageShares } from "@/lib/stores/workspace/projectLanguages.svelte";
  import { isLanguageMix, languageBarText } from "@/lib/workspace/language-bar";

  // A project row's GitHub-style language bar: a hairline along the row's foot,
  // one segment per language in proportion to its share of the source. Drawn
  // only for a project of two or more languages; the hover names them.
  const { path }: {
    path: string;
  } = $props();

  const shares = $derived(projectLanguageShares(path) ?? []);
  const isMix = $derived(isLanguageMix(shares));
  const tooltip = $derived.by(() => {
    if (!isMix) {
      return undefined;
    }

    return languageBarText(shares);
  });
</script>

<span
  class="language-bar"
  {@attach observeProjectLanguages({ path })}
  aria-hidden="true"
  data-tooltip={tooltip}
>
  {#if isMix}
    {#each shares as share (share.kind)}
      <span style:flex-grow={share.perMille} data-language={share.kind}></span>
    {/each}
  {/if}
</span>

<style>
  /* GitHub Linguist's colours, so the bar reads like a repository page's. An
     umbrella kind wears its most common language's colour: web → TypeScript,
     .NET → C#, Android → Kotlin. */
  .language-bar {
    position: absolute;
    inset-inline: 8px;
    inset-block-end: 1px;
    display: flex;
    gap: 1px;
    align-items: flex-end;
    block-size: 6px;

    > span {
      flex-basis: 0;
      block-size: 2px;
      background: var(--outline);
    }

    [data-language="web"]{ background: #3178c6; }
    [data-language="python"]{ background: #3572a5; }
    [data-language="java"]{ background: #b07219; }
    [data-language="go"]{ background: #00add8; }
    [data-language="rust"]{ background: #dea584; }
    [data-language="android"]{ background: #a97bff; }
    [data-language="cpp"]{ background: #f34b7d; }
    [data-language="dotnet"]{ background: #178600; }
    [data-language="php"]{ background: #4f5d95; }
    [data-language="ruby"]{ background: #701516; }
  }
</style>
