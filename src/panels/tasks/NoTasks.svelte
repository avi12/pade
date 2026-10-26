<script lang="ts">
  import type { TaskManifestDescriptor } from "@/lib/types";

  // What an empty Tasks panel says: no runnable tasks yet, and the manifests
  // that would bring some.
  const { manifests }: { manifests: readonly TaskManifestDescriptor[] } = $props();
</script>

<div class="empty">
  <p>No runnable tasks here. They appear as soon as the project carries one of these:</p>
  <ul class="manifest-list" hidden={manifests.length === 0}>
    {#each manifests as manifest (manifest.value)}
      <li>{manifest.value}</li>
    {/each}
  </ul>
</div>

<style>
  .empty {
    display: flex;
    flex-direction: column;
    gap: 10px;
    margin: 16px;
    color: var(--on-surface-variant);
    font-size: 0.8125rem;
    line-height: 1.5;

    p {
      margin: 0;
    }
  }

  /* The manifests the backend reads, straight from its registry - a wrapped set
     of tokens rather than a comma-run, because the list is long enough that a
     sentence stops being readable at panel width. */
  .manifest-list {
    display: flex;
    flex-wrap: wrap;
    gap: 6px;
    margin: 0;
    padding: 0;
    list-style: none;

    &[hidden] {
      display: none;
    }

    li {
      padding-block: 2px;
      padding-inline: 8px;
      border-radius: var(--radius-small);
      background: var(--surface-3);
      color: var(--on-surface);
      font-family: var(--font-monospace);
      font-size: 0.75rem;
    }
  }
</style>
