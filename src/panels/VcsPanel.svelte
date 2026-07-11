<script lang="ts">
  import { feed, os, vcs } from "@/lib/bridge";
  import CommitModal from "@/lib/CommitModal.svelte";
  import { formatCount } from "@/lib/format";
  import { setPanelHeader } from "@/lib/stores/sidePanel.svelte";
  import type { Commit, CommitDetail, StatusEntry } from "@/lib/types";
  import ChangesSection from "@/panels/vcs/ChangesSection.svelte";
  import "@/panels/vcs/chrome.css";
  import RestoreSection from "@/panels/vcs/RestoreSection.svelte";
  import type { UnlistenFn } from "@tauri-apps/api/event";
  import { onDestroy, onMount, tick } from "svelte";

  let entries = $state<StatusEntry[]>([]);
  let commits = $state<Commit[]>([]);
  let error = $state<string | null>(null);
  let unlisten: UnlistenFn | undefined;

  // Commit inspection — clicking a log row opens the detail modal; the remote URL
  // (fetched once) powers the "open on GitHub" links.
  let openCommit = $state<CommitDetail | null>(null);
  let remoteUrl = $state<string | null>(null);
  let logEl = $state<HTMLElement | null>(null);

  async function refresh() {
    try {
      [entries, commits] = await Promise.all([vcs.status(), vcs.log()]);
      error = null;
    } catch (e) {
      error = String(e);
      entries = [];
      commits = [];
    }
  }

  // ── Recent commits: open the detail modal, or Ctrl/Cmd-click → GitHub ───────
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

  // Debounced refresh so a burst of saves triggers one status fetch.
  let timer: ReturnType<typeof setTimeout> | undefined;
  function scheduleRefresh() {
    clearTimeout(timer);
    timer = setTimeout(refresh, 400);
  }

  onMount(async () => {
    await refresh();
    try {
      remoteUrl = await vcs.remoteUrl();
    } catch {
      remoteUrl = null;
    }
    unlisten = await feed.onChange(scheduleRefresh);
  });
  onDestroy(() => {
    unlisten?.();
    clearTimeout(timer);
  });

  // Publish the refresh action to the shared side-panel header.
  $effect(() => {
    setPanelHeader({
      count: null,
      refresh
    });
  });

  function fileLabel(count: number) {
    return `${formatCount(count)} file${count === 1 ? "" : "s"}`;
  }
</script>

<div class="vcs">
  {#if error}
    <p class="empty">Not a Git repository, or git is unavailable.</p>
  {:else}
    <div class="scroll">
      <RestoreSection />

      <ChangesSection {entries} />

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
    </div>
  {/if}
</div>

{#if openCommit}
  <CommitModal commit={openCommit} onclose={() => (openCommit = null)} {remoteUrl} />
{/if}

<style>
  .vcs {
    display: flex;
    flex-direction: column;
    height: 100%;
  }

  .scroll {
    display: flex;
    flex: 1;
    flex-direction: column;
    gap: 14px;
    overflow-y: auto;
    min-block-size: 0;
    padding: 8px 10px;
  }

  /* Shared panel chrome (group headers, sha, author line, empty state) lives
     in vcs/chrome.css so the section components share one copy. */

  /* "Restore a version" lives in vcs/RestoreSection.svelte; the status groups
     and inline diff live in vcs/ChangesSection.svelte. */

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
