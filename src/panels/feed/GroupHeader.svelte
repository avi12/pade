<script lang="ts">
  import { GroupRole } from "@/lib/feed/change-groups";
  import type { ChangeGroup } from "@/lib/feed/change-groups";
  import Icon from "@/lib/Icon.svelte";
  import { branchOfMember, pathOfMember, workspaceMembers } from "@/lib/stores/workspace/workspaceMembers.svelte";
  import { truncationTooltip } from "@/lib/ui/tooltip/truncation-tooltip";
  import { formatCount } from "@/lib/util/format";
  import { repositoryOfGroup } from "@/lib/workspace/members/workspace-members";
  import LineCounts from "@/panels/feed/card/LineCounts.svelte";

  // One project group's header: its role badge, name, git branch and totals.
  const { group }: { group: ChangeGroup } = $props();

  // The branch of the checkout the group's files actually live in, which for a
  // monorepo package is the workspace's own repo and for a nested checkout is
  // its own. Empty for a non-repo / detached-HEAD workspace. The branches ride
  // the shared member store, so the feed and the top bar can never report
  // different HEADs.
  const branch = $derived.by(() => {
    const repository = repositoryOfGroup({
      groupId: group.id,
      members: workspaceMembers()
    });
    if (!repository) {
      return undefined;
    }

    return branchOfMember(pathOfMember(repository));
  });

  const roleLabel = $derived.by(() => {
    if (group.role === GroupRole.Service) {
      return "SVC";
    }

    return group.role.toUpperCase();
  });
</script>

<header class="group-header">
  <span class="badge {group.role}">{roleLabel}</span>
  <span class="group-title">
    <!-- Ellipsizes in the narrow panel; grows a bubble only while clipped. -->
    <span class="group-name" {@attach truncationTooltip({ tooltip: group.name })}>{group.name}</span>
    {#if branch}
      <span class="group-branch"><Icon name="branch" size={10} />{branch}</span>
    {/if}
  </span>
  <span class="group-statistics">
    <LineCounts added={group.added} removed={group.removed} />
  </span>
  <span class="group-count">{formatCount(group.events.length)}</span>
</header>

<style>
  .group-header {
    display: flex;
    gap: 8px;
    align-items: center;
    padding: 4px;
  }

  /* The role reads as a tinted pill in the role's own colour - the fill a
     wash of the ink - so APP, LIB and SVC differ by hue, never by weight. */
  .badge {
    flex: none;
    padding-block: 2px;
    padding-inline: 8px;
    border-radius: var(--radius-full);
    font-weight: 800;
    font-size: 0.625rem;
    letter-spacing: 0.02em;

    &.app {
      background: var(--primary-container);
      color: var(--on-primary-container);
    }

    &.lib {
      background: var(--tertiary-wash);
      color: var(--tertiary);
    }

    &.service {
      background: var(--warning-wash);
      color: var(--warning);
    }
  }

  /* The name over its branch, one column, so the totals keep the row's end. */
  .group-title {
    display: flex;
    flex-direction: column;
    min-inline-size: 0;
    line-height: 1.2;
  }

  .group-name {
    overflow: hidden;
    font-family: var(--font-monospace);
    font-weight: 700;
    font-size: 0.8125rem;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  /* Git-branch subtitle under the group name - leads with the branch glyph, to
     match the switcher's branch chip; muted so the project name stays primary. */
  .group-branch {
    display: inline-flex;
    gap: 3px;
    align-items: center;
    overflow: hidden;
    color: var(--on-surface-variant);
    font-size: 0.625rem;
    white-space: nowrap;
  }

  .group-statistics {
    display: flex;
    flex: none;
    gap: 7px;
    margin-inline-start: auto;
    font-weight: 700;
    font-size: 0.6875rem;
  }

  .group-count {
    flex: none;
    min-inline-size: 1.25rem;
    padding-block: 1px;
    padding-inline: 7px;
    border-radius: var(--radius-full);
    background: var(--primary-container);
    color: var(--on-primary-container);
    font-weight: 700;
    font-size: 0.6875rem;
    text-align: center;
  }

  /* Cyberpunk sets every monospace name in Share Tech Mono, tracked a touch. */
  :global(:root[data-palette="cyberpunk"]) .group-name {
    letter-spacing: 0.02em;
  }
</style>
