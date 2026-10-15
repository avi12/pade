<script lang="ts">
  import type { ChangeGroup } from "@/lib/feed/change-groups";
  import { formatCount } from "@/lib/util/format";

  // The project chip row: "All" plus one chip per group, narrowing the feed to
  // the picked project. `null` is "All".
  let { groups, totalCount, activeGroupId = $bindable() }: {
    groups: ChangeGroup[];
    totalCount: number;
    activeGroupId: string | null;
  } = $props();
</script>

<div class="chips">
  <button class="chip" class:on={activeGroupId === null} onclick={() => (activeGroupId = null)}>
    All <span class="count">{formatCount(totalCount)}</span>
  </button>
  {#each groups as group (group.id)}
    <button
      class="chip"
      class:on={activeGroupId === group.id}
      onclick={() => (activeGroupId = group.id)}
    >{group.name} <span class="count">{formatCount(group.events.length)}</span></button>
  {/each}
</div>

<style>
  .chips {
    display: flex;
    flex-wrap: wrap;
    gap: 6px;
    padding-block: 10px 0;
    padding-inline: 10px;
  }

  .chip {
    display: inline-flex;
    gap: 6px;
    align-items: center;
    padding: 4px 11px;
    border: 1px solid var(--outline);
    border-radius: var(--radius-full);
    background: var(--surface-2);
    color: var(--on-surface-variant);
    font-size: 12px;
    cursor: pointer;
    transition:
      background 120ms var(--ease),
      color 120ms var(--ease);

    &:hover {
      background: var(--surface-3);
    }

    &.on {
      border-color: transparent;
      background: var(--primary-container);
      color: var(--on-primary-container);
      font-weight: 600;
    }

    .count {
      opacity: 75%;
    }
  }
</style>
