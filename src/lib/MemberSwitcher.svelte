<!--
  Which part of the workspace the top bar is pointed at.

  A workspace is not always one thing: it can hold nested checkouts (a multi-repo
  folder, a submodule) and declared packages (a monorepo's members). The backend
  finds both (src-tauri/members.rs) and `stores/workspaceMembers` holds the list;
  this is the one control that lets you say which of them "here" means — the
  branch the pill beside it reports, and the directory a new agent tab or the
  editor opens in.

  It only appears when there IS something to switch between: a plain single-repo
  workspace keeps the top bar exactly as it was.
-->
<script lang="ts">
  import Icon from "@/lib/Icon.svelte";
  import {
    activeMemberIn,
    branchOfMember,
    pathOfMember,
    selectMember,
    workspaceMembers
  } from "@/lib/stores/workspaceMembers.svelte";
  import { hasSeveralMembers, memberLabel } from "@/lib/workspace-members";

  const { root }: {
    /** The open project — the root member's own directory. */
    root: string;
  } = $props();

  const members = $derived(workspaceMembers());
  const entries = $derived(
    members.map(member => {
      const path = pathOfMember(member);
      return {
        path,
        label: memberLabel({
          root,
          member
        }),
        branch: branchOfMember(path),
        repository: member.repository,
        current: path === activeMemberIn(root)
      };
    })
  );
  const active = $derived(entries.find(entry => entry.current));
</script>

{#if hasSeveralMembers(members)}
  <span class="menu-host">
    <button
      style:anchor-name="--member-anchor"
      class="member-button menu-trigger"
      data-tooltip="Part of the workspace new agents and the editor open in"
      popovertarget="member-menu"
    >
      <Icon name={active?.repository ? "branch" : "folder"} size={13} />
      <span class="member-name">{active?.label ?? ""}</span>
      <span class="caret">▾</span>
    </button>
    <ul id="member-menu" style:position-anchor="--member-anchor" class="member-list popover-menu" popover>
      <li class="hint">Open agents and editors in</li>
      {#each entries as entry (entry.path)}
        <li>
          <button
            class="member-row"
            class:current={entry.current}
            onclick={() => selectMember(entry.path)}
            popovertarget="member-menu"
            popovertargetaction="hide"
          >
            <span class="member">
              <Icon name={entry.repository ? "branch" : "folder"} size={14} />{entry.label}
            </span>
            {#if entry.branch}
              <span class="member-branch">{entry.branch}</span>
            {/if}
          </button>
        </li>
      {/each}
    </ul>
  </span>
{/if}

<style>
  /* Layout-neutral wrapper so the shared .menu-host rule can hold the trigger
     active while #member-menu is open. */
  .menu-host {
    display: contents;
  }

  /* Sits between the project pill and the branch pill, and reads as the quieter
     of the two: the project names the workspace, this names the part of it. */
  .member-button {
    display: inline-flex;
    flex-shrink: 0;
    gap: 6px;
    align-items: center;
    max-inline-size: 220px;
    padding: 6px 11px;
    border: none;
    border-radius: var(--radius-full);
    background: var(--surface-2);
    color: var(--on-surface);
    font: inherit;
    font-weight: 600;
    font-size: 12px;
    cursor: pointer;

    &:hover {
      background: var(--surface-3);
    }

    .member-name {
      overflow: hidden;
      text-overflow: ellipsis;
      white-space: nowrap;
    }

    .caret {
      font-size: 10px;
      opacity: 70%;
    }
  }

  /* Shell comes from the shared .popover-menu; only width and anchor side
     live here. */
  .member-list {
    min-inline-size: 240px;
    position-area: bottom span-right;

    .hint {
      padding-block: 6px 4px;
      padding-inline: 10px;
      color: var(--on-surface-variant);
      font-weight: 700;
      font-size: 10px;
      letter-spacing: 0.08em;
      text-transform: uppercase;
    }

    .member-row {
      display: flex;
      gap: 12px;
      justify-content: space-between;
      align-items: center;
      inline-size: 100%;
      padding: 8px 10px;
      border: none;
      border-radius: var(--radius-small);
      background: transparent;
      color: var(--on-surface);
      font: inherit;
      font-size: 13px;
      text-align: start;
      cursor: pointer;

      &:hover {
        background: var(--surface-2);
      }

      &.current {
        background: var(--primary-container);
        color: var(--on-primary-container);
      }
    }

    .member {
      display: inline-flex;
      gap: 8px;
      align-items: center;
      overflow: hidden;
      text-overflow: ellipsis;
      white-space: nowrap;
    }

    /* The branch reads as data, not prose — mono, like every other ref in the
       app, and quiet enough that the member's own name still leads. */
    .member-branch {
      flex-shrink: 0;
      color: var(--on-surface-variant);
      font-family: var(--font-monospace);
      font-size: 11px;
    }
  }
</style>
