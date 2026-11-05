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
    ><span class="name">{group.name}</span> <span class="count badge">{formatCount(group.events.length)}</span></button>
  {/each}
</div>

<style>
  .chips {
    display: flex;
    flex-wrap: wrap;
    gap: 6px;
    padding-block: 8px 6px;
    padding-inline: 14px;
  }

  .chip {
    display: inline-flex;
    flex-shrink: 0;
    gap: 7px;
    align-items: center;
    padding-block: 5px;
    padding-inline: 11px;
    border: 1px solid var(--outline);
    border-radius: var(--radius-full);
    background: var(--surface-2);
    color: var(--on-surface);
    font-weight: 700;
    font-size: 0.75rem;
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
    }

    /* "All" leads the row a step wider, its total set quietly beside it. */
    &:first-child {
      gap: 6px;
      padding-inline: 12px;
    }

    .name {
      font-family: var(--font-monospace);
      white-space: nowrap;

      :global(:root[data-palette="cyberpunk"]) & {
        letter-spacing: 0.02em;
      }
    }

    .count {
      font-size: 0.625rem;
      opacity: 75%;
    }

    /* A project's own count is a small plate of its own. */
    .badge {
      padding-block: 0.5px;
      padding-inline: 6px;
      border-radius: var(--radius-full);
      background: var(--surface-3);
      color: var(--on-surface);
      opacity: 100%;
    }
  }
</style>
