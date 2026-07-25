<script lang="ts">
  import Icon from "@/lib/Icon.svelte";
  import { languageIcon } from "@/lib/language-icon";
  import { observeProjectKind, projectKind } from "@/lib/stores/projectKinds.svelte";

  const { path, size = 16 }: {
    path: string;
    size?: number;
  } = $props();
  const kind = $derived(projectKind(path));
  const icon = $derived(kind ? languageIcon(kind) : "folder");
</script>

<span
  class="project-kind-icon"
  {@attach observeProjectKind({ path })}
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
