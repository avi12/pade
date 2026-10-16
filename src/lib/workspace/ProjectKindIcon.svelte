<script lang="ts">
  import Icon from "@/lib/Icon.svelte";
  import { observeProjectKind, projectKind } from "@/lib/stores/workspace/projectKinds.svelte";
  import { observeProjectLanguages, projectLanguageShares } from "@/lib/stores/workspace/projectLanguages.svelte";
  import { languageIcon } from "@/lib/workspace/language-icon";

  const { path, size = 16 }: {
    path: string;
    size?: number;
  } = $props();
  // A project with no project file of any kind still has source: it wears its
  // biggest language.
  const kind = $derived(projectKind(path) ?? projectLanguageShares(path)?.[0]?.kind);
  const icon = $derived.by(() => {
    if (!kind) {
      return "folder";
    }

    return languageIcon(kind);
  });
</script>

<span
  class="project-kind-icon"
  {@attach observeProjectKind({ path })}
  {@attach observeProjectLanguages({ path })}
  aria-hidden="true"
  data-brand={kind ? icon : undefined}
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
