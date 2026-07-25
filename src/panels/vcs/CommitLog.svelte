<script lang="ts">
  import { os, vcs } from "@/lib/bridge";
  import CommitModal from "@/lib/CommitModal.svelte";
  import { formatCount } from "@/lib/format";
  import { repositoryCommitUrl } from "@/lib/repository-links";
  import type { Commit, CommitDetail } from "@/lib/types";
  import type { UnlistenFn } from "@tauri-apps/api/event";
  import { onDestroy, onMount, tick } from "svelte";

  // Recent commits: click a row to open the detail modal, Ctrl/Cmd-click (or
  // Ctrl/Cmd-Enter) to open the commit at its remote provider, arrow keys to move
  // through the log. This section owns the modal and its cached remote URL.
  const { project, commits }: {
    project: string;
    commits: Commit[];
  } = $props();

  let openCommit = $state<CommitDetail | null>(null);
  let remoteUrl = $state<string | null>(null);
  let logElement = $state<HTMLElement | null>(null);
  let unlistenGitState: UnlistenFn | undefined;

  async function loadRemoteUrl(workspace = project) {
    try {
      const next = await vcs.remoteUrl(workspace);
      if (workspace !== project) {
        return;
      }

      remoteUrl = next;
    } catch {
      if (workspace !== project) {
        return;
      }

      remoteUrl = null;
    }
  }

  onMount(async () => {
    await loadRemoteUrl();
    // A `git remote add`/`remove` (or git init) flips whether remote links
    // exist — re-read the remote the moment the live git state changes.
    unlistenGitState = await vcs.onStateChanged(async () => await loadRemoteUrl());
  });

  onDestroy(() => unlistenGitState?.());

  // This component stays mounted while the parent panel retargets. The commit
  // detail and remote from the old repository must not bleed into the next one.
  $effect(() => {
    if (!project) {
      return;
    }

    openCommit = null;
    loadRemoteUrl(project);
  });

  async function inspectCommit(commit: Commit) {
    try {
      openCommit = await vcs.commit(project, commit.id);
    } catch {
      openCommit = null;
    }
  }

  async function openCommitOnRemote(commit: Commit) {
    try {
      const base = remoteUrl ?? (await vcs.remoteUrl(project));
      remoteUrl = base;

      const commitUrl = repositoryCommitUrl({
        remoteUrl: base,
        commit: commit.id
      });
      if (commitUrl) {
        await os.openUrl(commitUrl);
      }
    } catch {
    // Opening the commit externally is best-effort — a missing remote or a
      // failed browser launch must not surface as an error here.
    }
  }

  async function focusCommit(index: number) {
    await tick();
    logElement?.querySelectorAll<HTMLElement>("[data-commit]")[index]?.focus();
  }

  function fileLabel(count: number) {
    return `${formatCount(count)} file${count === 1 ? "" : "s"}`;
  }
</script>

<section class="group log">
  <h3>Recent commits</h3>
  <ul bind:this={logElement} class="log-list">
    {#each commits as commit, index (commit.id)}
      <li>
        <button
          class="commit"
          aria-label="Commit {commit.short}: {commit.summary}, by {commit.author} {commit.when}"
          data-commit
          data-tooltip="Enter to view · Ctrl-click or Ctrl-Enter opens on remote"
          onclick={e => {
            const wantsRemote = e.ctrlKey || e.metaKey;
            if (wantsRemote) {
              e.preventDefault();
              openCommitOnRemote(commit);
              return;
            }

            inspectCommit(commit);
          }}
          onkeydown={e => {
            const isDown = e.key === "ArrowDown";
            const isUp = e.key === "ArrowUp";
            if (isDown || isUp) {
              e.preventDefault();
              const count = commits.length;
              const next = isDown ? (index + 1) % count : (index - 1 + count) % count;
              focusCommit(next);
              return;
            }

            const isOpenKey = e.key === "Enter" || e.key === " ";
            if (isOpenKey && (e.ctrlKey || e.metaKey)) {
              e.preventDefault();
              openCommitOnRemote(commit);
            }
          }}
        >
          <span class="c-top">
            <code class="commit-hash">{commit.short}</code>
            <span class="message">{commit.summary}</span>
          </span>
          <span class="c-bot">
            <span class="author-details">{commit.author} · {commit.when}</span>
            <span class="stats">
              <span class="fn">{fileLabel(commit.files)}</span>
              {#if commit.additions}
                <span class="add">+{formatCount(commit.additions)}</span>
              {/if}
              {#if commit.deletions}
                <span class="deletion">−{formatCount(commit.deletions)}</span>
              {/if}
            </span>
          </span>
        </button>
      </li>
    {/each}
  </ul>
</section>

{#if openCommit}
  <CommitModal commit={openCommit} onclose={() => (openCommit = null)} {project} {remoteUrl} />
{/if}

<style>
  .log-list {
    display: flex;
    flex-direction: column;
    gap: 2px;
    margin: 0;
    padding: 0;
    list-style: none;
  }

  .commit {
    display: flex;
    flex-direction: column;
    gap: 3px;
    inline-size: 100%;
    padding-block: 8px;
    padding-inline: 10px;
    border: none;
    border-radius: var(--radius-small);
    background: transparent;
    color: var(--on-surface);
    text-align: start;
    cursor: pointer;
    transition: background 120ms var(--ease);

    &:hover {
      background: var(--surface-2);
    }

    .c-top {
      display: flex;
      gap: 10px;
      align-items: baseline;
      inline-size: 100%;
    }

    .message {
      flex: 1;
      overflow: hidden;
      min-inline-size: 0;
      font-size: 13px;
      text-overflow: ellipsis;
      white-space: nowrap;
    }

    .c-bot {
      display: flex;
      gap: 10px;
      align-items: center;
      inline-size: 100%;
      color: var(--on-surface-variant);
      font-size: 11px;
    }

    .stats {
      display: flex;
      gap: 8px;
      align-items: center;
      margin-inline-start: auto;
      font-weight: 600;
      font-variant-numeric: tabular-nums;
    }

    .add {
      color: var(--tertiary);
    }

    .deletion {
      color: var(--critical);
    }
  }
</style>
