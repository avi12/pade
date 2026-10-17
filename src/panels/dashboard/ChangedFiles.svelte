<script lang="ts">
  import { baseName } from "@/lib/util/paths";

  // A project card's latest changed files, by name, each with its full path on
  // hover. Nothing renders while no file has changed.
  const { files }: {
    files: readonly string[];
  } = $props();
</script>

{#if files.length > 0}
  <ul class="files">
    {#each files as file (file)}
      <li data-tooltip={file}>{baseName(file)}</li>
    {/each}
  </ul>
{/if}

<style>
  .files {
    display: flex;
    flex-wrap: wrap;
    gap: 6px;
    margin: 0;
    padding: 0;
    list-style: none;

    li {
      padding: 2px 7px;
      border-radius: var(--radius-small);
      background: var(--surface-2);
      color: var(--on-surface-variant);
      font-family: var(--font-monospace);
      font-size: 11px;
    }
  }
</style>
