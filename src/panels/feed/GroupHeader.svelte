<script lang="ts">
  import { GroupRole } from "@/lib/feed/change-groups";
  import type { ChangeGroup } from "@/lib/feed/change-groups";
  import Icon from "@/lib/Icon.svelte";
  import { branchOfMember, pathOfMember, workspaceMembers } from "@/lib/stores/workspace/workspaceMembers.svelte";
  import { truncationTooltip } from "@/lib/ui/tooltip/truncation-tooltip";
  import { formatCount } from "@/lib/util/format";
  import { repositoryOfGroup } from "@/lib/workspace/members/workspace-members";
  import LineCounts from "@/panels/feed/LineCounts.svelte";

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
  <!-- Ellipsizes in the narrow panel; grows a bubble only while clipped. -->
  <span class="group-name" {@attach truncationTooltip({ tooltip: group.name })}>{group.name}</span>
  {#if branch}
    <span class="group-branch"><Icon name="branch" size={12} />{branch}</span>
  {/if}
  <span class="group-statistics">
    <LineCounts added={group.added} removed={group.removed} />
  </span>
  <span class="group-count">{formatCount(group.events.length)}</span>
</header>

<style>
  .group-header {
    display: flex;
    gap: 9px;
    align-items: center;
    padding-block: 2px 8px;
    padding-inline: 6px;
  }

  .badge {
    flex: none;
    padding: 2px 6px;
    border-radius: var(--radius-small);
    color: #ffffff;
    font-family: var(--font-monospace);
    font-weight: 700;
    font-size: 0.5938rem;
    letter-spacing: 0.05em;

    &.app {
      background: #3b6fe0;
    }

    &.lib {
      background: #12a58a;
    }

    &.service {
      background: #c8871a;
    }
  }

  .group-name {
    overflow: hidden;
    font-weight: 700;
    font-size: 0.8125rem;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  /* Git-branch subtitle beside the group name - leads with the branch glyph, to
     match the switcher's branch chip; muted so the project name stays primary. */
  .group-branch {
    display: inline-flex;
    flex: none;
    gap: 3px;
    align-items: center;
    color: var(--on-surface-variant);
    font-family: var(--font-monospace);
    font-size: 0.6875rem;
    white-space: nowrap;
  }

  .group-statistics {
    display: flex;
    gap: 8px;
    margin-inline-start: auto;
    font-family: var(--font-monospace);
    font-weight: 600;
    font-size: 0.6875rem;
  }

  .group-count {
    flex: none;
    min-inline-size: 20px;
    padding: 1px 7px;
    border-radius: var(--radius-full);
    background: var(--surface-2);
    color: var(--on-surface-variant);
    font-size: 0.6563rem;
    text-align: center;
  }
</style>
