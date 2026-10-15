<script lang="ts">
  import { formatCount, formatQuantity } from "@/lib/format";

  // A commit's size - its file count and the lines it added and removed - as
  // sibling spans, so the commit log row and the commit modal's metadata line
  // lay them out with their own flex gap.
  const { files, additions, deletions }: {
    files: number;
    additions: number;
    deletions: number;
  } = $props();
</script>

<span>{formatQuantity({
  count: files,
  noun: "file"
})}</span>
{#if additions}
  <span class="add">+{formatCount(additions)}</span>
{/if}
{#if deletions}
  <span class="deletion">−{formatCount(deletions)}</span>
{/if}

<style>
  .add {
    color: var(--tertiary);
    font-weight: 600;
  }

  .deletion {
    color: var(--critical);
    font-weight: 600;
  }
</style>
