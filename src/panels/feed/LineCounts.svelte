<script lang="ts">
  import { formatCount } from "@/lib/util/format";

  // A change's line deltas - "+12 −3" - each shown only when non-zero. The
  // caller's wrapper lays them out.
  const { added, removed }: {
    added: number;
    removed: number;
  } = $props();
</script>

{#if added}
  <span class="add">+{formatCount(added)}</span>
{/if}
{#if removed}
  <span class="deletion">−{formatCount(removed)}</span>
{/if}

<style>
  .add {
    color: var(--tertiary);
  }

  .deletion {
    color: var(--critical);
  }
</style>
