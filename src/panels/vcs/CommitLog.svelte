<script lang="ts">
  import { os, vcs } from "@/lib/bridge";
  import CommitModal from "@/lib/CommitModal.svelte";
  import { formatCount } from "@/lib/format";
  import type { Commit, CommitDetail } from "@/lib/types";
  import { onMount, tick } from "svelte";

  // Recent commits: click a row to open the detail modal, Ctrl/Cmd-click (or
  // Ctrl/Cmd-Enter) to open the commit on GitHub, arrow keys to move through
  // the log. This section owns the modal and the remote URL powering the
  // GitHub links.
  const { commits }: {
    commits: Commit[];
  } = $props();

  let openCommit = $state<CommitDetail | null>(null);
  let remoteUrl = $state<string | null>(null);
  let logEl = $state<HTMLElement | null>(null);

  onMount(async () => {
    try {
      remoteUrl = await vcs.remoteUrl();
    } catch {
      remoteUrl = null;
    }
  });

  async function inspectCommit(commit: Commit) {
    try {
      openCommit = await vcs.commit(commit.id);
    } catch {
      openCommit = null;
    }
  }

  async function openCommitOnGithub(commit: Commit) {
    const base = remoteUrl ?? (await vcs.remoteUrl());
    remoteUrl = base;

    if (base) {
      void os.openUrl(`${base}/commit/${commit.id}`);
    }
  }

  function onCommitClick(event: MouseEvent, commit: Commit) {
    const wantsGithub = event.ctrlKey || event.metaKey;
    if (wantsGithub) {
      event.preventDefault();
      void openCommitOnGithub(commit);
      return;
    }

    void inspectCommit(commit);
  }

  // Arrow-key navigation across the log; Ctrl/Cmd-Enter opens the commit on GitHub.
  function onCommitKey(event: KeyboardEvent, index: number, commit: Commit) {
    const isDown = event.key === "ArrowDown";
    const isUp = event.key === "ArrowUp";
    if (isDown || isUp) {
      event.preventDefault();
      const count = commits.length;
      const next = isDown ? (index + 1) % count : (index - 1 + count) % count;
      void focusCommit(next);
      return;
    }

    const isOpenKey = event.key === "Enter" || event.key === " ";
    if (isOpenKey && (event.ctrlKey || event.metaKey)) {
      event.preventDefault();
      void openCommitOnGithub(commit);
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
          onclick={event => onCommitClick(event, c)}
          onkeydown={event => onCommitKey(event, index, c)}
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
  <CommitModal commit={openCommit} onclose={() => (openCommit = null)} {remoteUrl} />
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
