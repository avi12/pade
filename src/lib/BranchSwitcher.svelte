<!--
  The checked-out branch, and the one control that changes it.

  The pill names the branch HEAD is on for the part of the workspace the top bar
  points at (`MemberSwitcher` picks that part; `App` reads the branches for it).
  Opening it lists every branch the repo can switch to: the local ones, then the
  ones only the remote has so far — `git switch <name>` creates the local
  tracking branch for those, so a fresh clone can reach work it never checked out.

  A switch git refuses — uncommitted work in the way, or a branch already held by
  another worktree — surfaces git's own sentence in the toast, because that
  sentence names the obstacle.
-->
<script lang="ts">
  import { vcs } from "@/lib/bridge";
  import { errorHeadline } from "@/lib/error-text";
  import Icon from "@/lib/Icon.svelte";
  import { rovingMenu } from "@/lib/roving-menu";
  import { showToast } from "@/lib/stores/toast.svelte";

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

  async function switchTo(branch: string): Promise<void> {
    if (branch === current) {
      return;
    }

    try {
      const landed = await vcs.switchBranch({
        cwd,
        branch
      });
      await onrefresh();
      showToast(`Switched to ${landed}`);
    } catch (error) {
      showToast(
        errorHeadline({
          error,
          fallback: `Could not switch to ${branch}.`
        })
      );
    }
  }
</script>

{#if current}
  <span class="menu-host">
    <button
      style:anchor-name="--branch-anchor"
      class="branch-pill menu-trigger"
      data-tooltip={hasRemote
        ? "Switch branch · Ctrl-click opens on remote"
        : "Switch branch"}
      onclick={async e => {
        // Ctrl/Cmd-click is the shortcut for the menu's own open-on-remote row.
        // Cancelling the click cancels the popover this button would otherwise
        // toggle, so the menu stays shut while the browser opens.
        if (hasRemote && (e.ctrlKey || e.metaKey)) {
          e.preventDefault();
          await onopenremote();
        }
      }}
      popovertarget="branch-menu"
    >
      <span class="branch-pill-icon" aria-hidden="true"><Icon name="branch" size={13} /></span>
      <span class="branch-pill-name">{current}</span>
      <span class="caret">▾</span>
    </button>

    <ul
      id="branch-menu"
      style:position-anchor="--branch-anchor"
      class="branch-menu popover-menu scroll-fade"
      {@attach rovingMenu}
      ontoggle={async e => {
        if ((e as ToggleEvent).newState === "open") {
          await onrefresh();
        }
      }}
      popover
    >
      <li class="menu-separator">Switch branch</li>
      {#each branches as branch (branch)}
        <li>
          <button
            class="branch-row"
            class:current={branch === current}
            onclick={async () => await switchTo(branch)}
            popovertarget="branch-menu"
            popovertargetaction="hide"
          >
            <span class="mark" aria-hidden="true">
              {#if branch === current}
                <Icon name="check" size={13} />
              {/if}
            </span>
            <span class="branch-name">{branch}</span>
          </button>
        </li>
      {/each}

      {#if remoteBranches.length > 0}
        <li class="menu-divider" role="separator"></li>
        <li class="menu-separator">On the remote — checks out a local copy</li>
        {#each remoteBranches as branch (branch)}
          <li>
            <button
              class="branch-row remote"
              onclick={async () => await switchTo(branch)}
              popovertarget="branch-menu"
              popovertargetaction="hide"
            >
              <span class="mark" aria-hidden="true"><Icon name="git" size={13} /></span>
              <span class="branch-name">{branch}</span>
            </button>
          </li>
        {/each}
      {/if}

      <li class="menu-divider" role="separator"></li>
      {#if hasRemote}
        <li>
          <button
            class="action-row"
            onclick={async () => await onopenremote()}
            popovertarget="branch-menu"
            popovertargetaction="hide"
          ><Icon name="external" size={14} />Open branch on remote</button>
        </li>
      {/if}
      <li>
        <button
          class="action-row"
          onclick={() => onopengit()}
          popovertarget="branch-menu"
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
     chip) — click opens the branch list. */
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

    .branch-pill-icon {
      display: inline-flex;
      flex-shrink: 0;
      color: var(--on-surface-variant);
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

  /* Shell comes from the shared .popover-menu; only width, anchor side and the
     row styles live here. Long branch lists scroll inside the menu. */
  .branch-menu {
    overflow-y: auto;
    max-block-size: min(60vh, 420px);
    min-inline-size: 240px;
    position-area: bottom span-right;

    .menu-separator {
      margin-block: 6px 2px;
      padding-block: 2px 4px;
      padding-inline: 10px;
      color: var(--on-surface-variant);
      font-weight: 700;
      font-size: 10px;
      letter-spacing: 0.08em;
      text-transform: uppercase;
    }

    /* Hairline between the local branches, the remote-only ones, and the
       actions below them. */
    .menu-divider {
      block-size: 1px;
      margin-block: 6px;
      margin-inline: 8px;
      background: var(--outline);
    }

    .branch-row,
    .action-row {
      display: flex;
      gap: 8px;
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
      transition: color 120ms var(--ease), background 120ms var(--ease);

      &:hover {
        background: var(--primary-container);
        color: var(--on-primary-container);
      }
    }

    /* A ref reads as data — the mono face, like every other branch name in the
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

    /* The two prose actions under the refs — sentence case, not mono. */
    .action-row {
      color: var(--on-surface-variant);
      font-size: 12px;
    }
  }
</style>
