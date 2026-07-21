<script lang="ts">
  import { os, vcs } from "@/lib/bridge";
  import CommitModal from "@/lib/CommitModal.svelte";
  import { formatCount } from "@/lib/format";
  import type { Commit, CommitDetail } from "@/lib/types";
  import type { UnlistenFn } from "@tauri-apps/api/event";
  import { onDestroy, onMount, tick } from "svelte";

  // Recent commits: click a row to open the detail modal, Ctrl/Cmd-click (or
  // Ctrl/Cmd-Enter) to open the commit on GitHub, arrow keys to move through
  // the log. This section owns the modal and the remote URL powering the
  // GitHub links.
  const { project, commits }: {
    project: string;
    commits: Commit[];
  } = $props();

  let openCommit = $state<CommitDetail | null>(null);
  let remoteUrl = $state<string | null>(null);
  let logEl = $state<HTMLElement | null>(null);
  let unlistenGitState: UnlistenFn | undefined;

  async function loadRemoteUrl(workspace = project) {
    try {
      const next = await vcs.remoteUrl(workspace);
      if (workspace === project) {
        remoteUrl = next;
      }
    } catch {
      if (workspace === project) {
        remoteUrl = null;
      }
    }
  }

  onMount(async () => {
    await loadRemoteUrl();
    // A `git remote add`/`remove` (or git init) flips whether GitHub links
    // exist — re-read the remote the moment the live git state changes.
    unlistenGitState = await vcs.onStateChanged(() => void loadRemoteUrl());
  });

  onDestroy(() => unlistenGitState?.());

  // This component stays mounted while the parent panel retargets. The commit
  // detail and remote from the old repository must not bleed into the next one.
  $effect(() => {
    if (project) {
      openCommit = null;
      void loadRemoteUrl(project);
    }
  });

  async function inspectCommit(commit: Commit) {
    try {
      openCommit = await vcs.commit(project, commit.id);
    } catch {
      openCommit = null;
    }
  }

  async function openCommitOnGithub(commit: Commit) {
    try {
      const base = remoteUrl ?? (await vcs.remoteUrl(project));
      remoteUrl = base;

      if (base) {
        await os.openUrl(`${base}/commit/${commit.id}`);
      }
    } catch {
    // Opening the commit externally is best-effort — a missing remote or a
      // failed browser launch must not surface as an error here.
    }
  }

  async function focusCommit(index: number) {
    await tick();
    logEl?.querySelectorAll<HTMLElement>("[data-commit]")[index]?.focus();
  }

  function fileLabel(count: number) {
    return `${formatCount(count)} file${count === 1 ? "" : "s"}`;
  }
</script>

<section class="group log">
  <h3>Recent commits</h3>
  <ul bind:this={logEl} class="log-list">
    {#each commits as c, index (c.id)}
      <li>
        <button
          class="commit"
          aria-label="Commit {c.short}: {c.summary}, by {c.author} {c.when}"
          data-commit
          data-tooltip="Enter to view · Ctrl-click or Ctrl-Enter opens on GitHub"
          onclick={e => {
            const wantsGithub = e.ctrlKey || e.metaKey;
            if (wantsGithub) {
              e.preventDefault();
              openCommitOnGithub(c);
              return;
            }

            inspectCommit(c);
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
              openCommitOnGithub(c);
            }
          }}
        >
          <span class="c-top">
            <code class="sha">{c.short}</code>
            <span class="msg">{c.summary}</span>
          </span>
          <span class="c-bot">
            <span class="by">{c.author} · {c.when}</span>
            <span class="stats">
              <span class="fn">{fileLabel(c.files)}</span>
              {#if c.additions}
                <span class="add">+{formatCount(c.additions)}</span>
              {/if}
              {#if c.deletions}
                <span class="del">−{formatCount(c.deletions)}</span>
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

    .msg {
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

    .del {
      color: var(--critical);
    }
  }
</style>
