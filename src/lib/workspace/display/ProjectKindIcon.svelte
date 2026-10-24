<script lang="ts">
  import Icon from "@/lib/Icon.svelte";
  import type { IconName } from "@/lib/Icon.svelte";
  import { observeProjectKind, projectKind } from "@/lib/stores/workspace/projectKinds.svelte";
  import { observeProjectLanguages, projectLanguageShares } from "@/lib/stores/workspace/projectLanguages.svelte";
  import { languageIcon, languageIconOf } from "@/lib/workspace/display/language-icon";

  // A project row's language icon: the logo of the language most of its source
  // is in (a Gradle project of Kotlin shows Kotlin, not Java). Until that count
  // lands - and for a folder with no countable source - the project files'
  // guess stands in, then a plain folder.
  const FOLDER_ICON: IconName = "folder";

  const { path, size = 16 }: {
    path: string;
    size?: number;
  } = $props();
  const kind = $derived(projectKind(path));
  const biggestLanguage = $derived(projectLanguageShares(path)?.[0]?.language);
  const icon = $derived.by(() => {
    if (biggestLanguage) {
      return languageIconOf(biggestLanguage);
    }

    if (kind) {
      return languageIcon(kind);
    }

    return FOLDER_ICON;
  });
  const isLanguageKnown = $derived(Boolean(biggestLanguage ?? kind));
</script>

<span
  class="project-kind-icon"
  {@attach observeProjectKind({ path })}
  {@attach observeProjectLanguages({ path })}
  aria-hidden="true"
  data-brand={isLanguageKnown ? icon : undefined}
><Icon name={icon} {size} /></span>

<style>
  .project-kind-icon {
    display: inline-grid;
    flex: 0 0 auto;
    place-items: center;
    block-size: 20px;
    inline-size: 20px;
    color: var(--brand-color, var(--on-surface-variant));
  }
</style>
