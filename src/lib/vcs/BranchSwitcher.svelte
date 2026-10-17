<!--
  The checked-out branch, and the one control that changes it.

  The pill names the branch HEAD is on for the part of the workspace the top bar
  points at (`MemberSwitcher` picks that part; `App` reads the branches for it).
  Opening it lists every branch the repo can switch to: the local ones, then the
  ones only the remote has so far - `git switch <name>` creates the local
  tracking branch for those, so a fresh clone can reach work it never checked out.

  A switch git refuses - uncommitted work in the way, or a branch already held by
  another worktree - surfaces git's own sentence in the toast, because that
  sentence names the obstacle.
-->
<script lang="ts">
  import Icon from "@/lib/Icon.svelte";
  import { Command, sendMessage } from "@/lib/ipc/messaging";
  import { showToast } from "@/lib/stores/ui/toast.svelte";
  import { rovingMenu } from "@/lib/ui/roving-menu";
  import { ToggleState } from "@/lib/ui/toggle-state";
  import { errorHeadline } from "@/lib/util/error-text";

  const {
    cwd,
    current,
    branches,
    remoteBranches,
    hasRemote,
    onrefresh,
    onopengit,
    onopenremote
  }: {
    /** The repository the pill reports on and the switch acts on. */
    cwd: string;
    /** Branch HEAD is on; the pill is absent on a non-repo or detached HEAD. */
    current: string;
    /** Local branches, as `vcs_branches` lists them. */
    branches: string[];
    /** Branches only a remote has yet, under their short name. */
    remoteBranches: string[];
    /** Does the repo have a remote? Gates the open-on-remote affordances. */
    hasRemote: boolean;
    /** Re-read the branch state: on opening the menu (a branch an agent just
     *  created in a terminal moves no ref the git-state watch sees) and again
     *  once a switch has moved HEAD. */
    onrefresh: () => Promise<void>;
    /** Show the Git panel. */
    onopengit: () => void;
    /** Open the current branch on its remote provider. */
    onopenremote: () => Promise<void>;
  } = $props();

  const MENU_ID = "branch-menu";
  const MENU_ANCHOR = "--branch-anchor";

  const pillTooltip = $derived.by(() => {
    if (hasRemote) {
      return "Switch branch · Ctrl-click opens on remote";
    }

    return "Switch branch";
  });
</script>

<!-- One row per branch: a local one is checked when it is HEAD's, a remote-only
     one wears the git glyph for the fetch its switch implies. -->
{#snippet branchRow(branch: string, isRemote: boolean)}
  {@const isCurrent = branch === current}
  <li>
    <button
      class="menu-item branch-row"
      class:current={isCurrent}
      class:remote={isRemote}
      onclick={async () => {
        if (isCurrent) {
          return;
        }

        try {
          const landed = await sendMessage(Command.vcsSwitchBranch, {
            cwd,
            branch
          });
          await onrefresh();
          showToast(`Switched to ${landed}`);
        } catch (error) {
          showToast(
            errorHeadline({
              error,
              fallback: `Could not switch to ${branch}`
            })
          );
        }
      }}
      popovertarget={MENU_ID}
      popovertargetaction="hide"
    >
      <span class="mark" aria-hidden="true">
        {#if isRemote}
          <Icon name="git" size={13} />
        {:else if isCurrent}
          <Icon name="check" size={13} />
        {/if}
      </span>
      <span class="branch-name">{branch}</span>
    </button>
  </li>
{/snippet}

{#if current}
  <span class="menu-host">
    <button
      style:anchor-name={MENU_ANCHOR}
      class="branch-pill menu-trigger"
      data-tooltip={pillTooltip}
      onclick={async e => {
        // Ctrl/Cmd-click is the shortcut for the menu's own open-on-remote row.
        // Cancelling the click cancels the popover this button would otherwise
        // toggle, so the menu stays shut while the browser opens.
        const isOpenOnRemoteClick = hasRemote && (e.ctrlKey || e.metaKey);
        if (isOpenOnRemoteClick) {
          e.preventDefault();
          await onopenremote();
        }
      }}
      popovertarget={MENU_ID}
    >
      <span class="branch-pill-icon" aria-hidden="true"><Icon name="branch" size={13} /></span>
      <span class="branch-pill-name">{current}</span>
      {#if hasRemote}
        <!-- Says the branch lives on a remote too, which a Ctrl-click opens. -->
        <span class="branch-pill-remote" aria-hidden="true"><Icon name="external" size={11} /></span>
      {/if}
      <span class="caret">▾</span>
    </button>

    <ul
      id={MENU_ID}
      style:position-anchor={MENU_ANCHOR}
      class="branch-menu popover-menu scroll-fade"
      {@attach rovingMenu}
      ontoggle={async e => {
        if ((e as ToggleEvent).newState === ToggleState.Open) {
          await onrefresh();
        }
      }}
      popover
    >
      <li class="menu-separator">Switch branch</li>
      {#each branches as branch (branch)}
        {@render branchRow(branch, false)}
      {/each}

      {#if remoteBranches.length > 0}
        <li class="menu-divider" role="separator"></li>
        <li class="menu-separator">On the remote - checks out a local copy</li>
        {#each remoteBranches as branch (branch)}
          {@render branchRow(branch, true)}
        {/each}
      {/if}

      <li class="menu-divider" role="separator"></li>
      {#if hasRemote}
        <li>
          <button
            class="menu-item action-row"
            onclick={async () => await onopenremote()}
            popovertarget={MENU_ID}
            popovertargetaction="hide"
          ><Icon name="external" size={14} />Open branch on remote</button>
        </li>
      {/if}
      <li>
        <button
          class="menu-item action-row"
          onclick={() => onopengit()}
          popovertarget={MENU_ID}
          popovertargetaction="hide"
        ><Icon name="git" size={14} />Open Git panel</button>
      </li>
    </ul>
  </span>
{/if}

<style>
  /* Layout-neutral wrapper so the shared .menu-host rule can hold the trigger
     active while #branch-menu is open. */
  .menu-host {
    display: contents;
  }

  /* The checked-out branch, pilled beside the project button (design's branch
     chip) - click opens the branch list. */
  .branch-pill {
    display: inline-flex;
    flex-shrink: 0;
    gap: 6px;
    align-items: center;
    min-inline-size: 0;
    max-inline-size: 14rem;
    padding-block: 4px;
    padding-inline: 9px 10px;
    border: none;
    border-radius: var(--radius-full);
    background: var(--surface-2);
    color: var(--on-surface);
    font: inherit;
    white-space: nowrap;
    cursor: pointer;
    transition: background 150ms var(--ease);

    &:hover {
      background: var(--surface-3);
    }

    .branch-pill-icon,
    .branch-pill-remote {
      display: inline-flex;
      flex-shrink: 0;
      color: var(--on-surface-variant);
    }

    .branch-pill-remote {
      opacity: 70%;
      transition: opacity 150ms var(--ease);
    }

    &:hover .branch-pill-remote {
      opacity: 100%;
    }

    .branch-pill-name {
      overflow: hidden;
      min-inline-size: 0;
      font-family: var(--font-monospace);
      font-weight: 700;
      font-size: 12px;
      text-overflow: ellipsis;
    }

    /* Flipped by the shared .menu-host rule while the menu is open. */
    .caret {
      flex-shrink: 0;
      font-size: 10px;
      opacity: 70%;
      transition: rotate 150ms var(--ease);
    }
  }

  /* Shell and rows come from the shared .popover-menu and .menu-item; only
     width, anchor side and what each row adds live here. Long branch lists
     scroll inside the menu. */
  .branch-menu {
    overflow-y: auto;
    max-block-size: min(60vh, 420px);
    min-inline-size: 240px;
    position-area: bottom span-right;

    .menu-item {
      gap: 8px;
    }

    /* A ref reads as data - the mono face, like every other branch name in the
       app. The check column keeps every name on the same start edge. */
    .branch-row {
      font-family: var(--font-monospace);

      &.current {
        font-weight: 700;
      }
    }

    .mark {
      display: inline-flex;
      flex-shrink: 0;
      justify-content: center;
      inline-size: 13px;
      color: var(--primary);
    }

    /* Not here yet: the git glyph marks a branch the switch has to fetch out of
       the remote first. */
    .remote .mark {
      color: var(--tertiary);
    }

    .branch-name {
      overflow: hidden;
      min-inline-size: 0;
      text-overflow: ellipsis;
      white-space: nowrap;
    }

    /* The two prose actions under the refs - sentence case, not mono. */
    .action-row {
      color: var(--on-surface-variant);
      font-size: 12px;

      &:hover {
        color: var(--on-primary-container);
      }
    }
  }
</style>
