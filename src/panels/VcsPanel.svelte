<script lang="ts">
  import { feed, os, vcs } from "@/lib/bridge";
  import ColorText from "@/lib/ColorText.svelte";
  import CommitModal from "@/lib/CommitModal.svelte";
  import { DiffKind, parseDiff, toSplitRows } from "@/lib/diff";
  import { formatCount, formatPercent } from "@/lib/format";
  import Icon from "@/lib/Icon.svelte";
  import { baseName } from "@/lib/paths";
  import { effective } from "@/lib/prefs.svelte";
  import { setPanelHeader } from "@/lib/stores/sidePanel.svelte";
  import { DiffStyle, VcsKind } from "@/lib/types";
  import type { Commit, CommitDetail, RestoreCandidate, StatusEntry } from "@/lib/types";
  import { parseInput, RestoreQuery } from "@/lib/validate";
  import "@/panels/vcs/chrome.css";
  import type { UnlistenFn } from "@tauri-apps/api/event";
  import { onDestroy, onMount, tick } from "svelte";

  let entries = $state<StatusEntry[]>([]);
  let commits = $state<Commit[]>([]);
  let error = $state<string | null>(null);
  let selected = $state<StatusEntry | null>(null);
  let diff = $state("");
  let unlisten: UnlistenFn | undefined;

  // Commit inspection — clicking a log row opens the detail modal; the remote URL
  // (fetched once) powers the "open on GitHub" links.
  let openCommit = $state<CommitDetail | null>(null);
  let remoteUrl = $state<string | null>(null);
  let logEl = $state<HTMLElement | null>(null);

  // Restore a version — natural-language → ranked prior commits → checkout.
  let restoreQuery = $state("");
  let candidates = $state<RestoreCandidate[]>([]);
  let restoreError = $state<string | null>(null);
  let restoreDone = $state<string | null>(null);
  let searching = $state(false);

  // Grouped views — agent-oriented review lives in the "unreviewed" (unstaged)
  // group; "approve" moves entries into the staged group.
  const unstaged = $derived(entries.filter(e => !e.staged));
  const staged = $derived(entries.filter(e => e.staged));

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

  async function open(entry: StatusEntry) {
    selected = entry;
    const isUntracked = entry.kind === VcsKind.enum.untracked;
    diff = isUntracked
      ? "(new file — not yet tracked)"
      : await vcs.diff({
        path: entry.path,
        staged: entry.staged
      });
  }

  async function runRestore() {
    const query = parseInput({
      schema: RestoreQuery,
      raw: restoreQuery
    });
    if (!query) {
      return;
    }

    searching = true;
    restoreError = null;
    restoreDone = null;
    try {
      candidates = await vcs.restoreCandidates({ query });
    } catch (e) {
      restoreError = String(e);
      candidates = [];
    } finally {
      searching = false;
    }
  }

  function onRestoreKey(event: KeyboardEvent) {
    const isSubmit = event.key === "Enter";
    if (isSubmit) {
      void runRestore();
    }
  }

  async function checkout(candidate: RestoreCandidate) {
    restoreError = null;
    try {
      const branch = await vcs.restoreCheckout(candidate.id);
      restoreDone = branch;
      candidates = [];
    } catch (e) {
      restoreError = String(e);
    }
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

  // Diff rendering funnels through the shared parser (DRY) — one authoritative
  // line classifier for both the Change Feed and this panel. The split view is
  // derived from the same parsed lines (reuse `toSplitRows`, never re-implement).
  const diffLines = $derived(parseDiff(diff));
  const splitRows = $derived(toSplitRows(diffLines));
  const isSplit = $derived(effective.diffStyle === DiffStyle.enum.split);

  // Confidence as a 0..100 percentage — scores run 0..≈1.5, clamped for display.
  function confidencePct(score: number): number {
    return Math.round(Math.min(score / 1.5, 1) * 100);
  }
</script>

<div class="vcs">
  {#if error}
    <p class="empty">Not a Git repository, or git is unavailable.</p>
  {:else}
    <div class="scroll">
      <section class="restore">
        <h3 class="eyebrow"><span class="lead"><Icon name="history" /></span> Restore a version</h3>
        <div class="restore-input">
          <input
            aria-label="Restore to a previous version"
            onkeydown={onRestoreKey}
            placeholder="e.g. last working version, before the meter change"
            type="text"
            bind:value={restoreQuery}
          />
          <button class="go" disabled={searching} onclick={runRestore}>Restore</button>
        </div>
        <p class="hint">
          PADE reads your local edit history and runs <code>git bisect</code> to trace back to the matching version.
        </p>

        {#if restoreError}
          <p class="restore-msg crit">{restoreError}</p>
        {/if}
        {#if restoreDone}
          <p class="restore-msg ok">Checked out on <code>{restoreDone}</code></p>
        {/if}

        {#if candidates.length}
          <ul class="candidates">
            {#each candidates as c (c.id)}
              <li>
                <button class="candidate" onclick={() => checkout(c)}>
                  <div class="cand-top">
                    <code class="sha">{c.short}</code>
                    <span class="summary">{c.summary}</span>
                  </div>
                  <div class="cand-bot">
                    <span class="by">{c.author} · {c.when}</span>
                    <span class="conf" aria-label="Match confidence">
                      <span class="bar"><span style:inline-size="{confidencePct(c.score)}%" class="fill"></span></span>
                      <span class="pct">{formatPercent(confidencePct(c.score))}</span>
                    </span>
                  </div>
                </button>
              </li>
            {/each}
          </ul>
        {:else if searching}
          <p class="hint">Searching your history…</p>
        {:else if restoreQuery.trim() && !restoreDone && !restoreError}
          <p class="hint">No matching version found — try describing it differently.</p>
        {/if}
      </section>

      {#if unstaged.length}
        <section class="group">
          <h3><span class="dot agent"></span> Unreviewed <span class="n">{formatCount(unstaged.length)}</span></h3>
          {#each unstaged as e (e.path)}
            <button class="row" class:sel={selected?.path === e.path} onclick={() => open(e)}>
              <span class="k {e.kind}">{e.kind[0].toUpperCase()}</span>
              <span class="fname">{baseName(e.path)}</span>
            </button>
          {/each}
        </section>
      {/if}

      {#if staged.length}
        <section class="group">
          <h3><span class="dot staged"></span> Staged <span class="n">{formatCount(staged.length)}</span></h3>
          {#each staged as e (e.path)}
            <button class="row" class:sel={selected?.path === e.path} onclick={() => open(e)}>
              <span class="k {e.kind}">{e.kind[0].toUpperCase()}</span>
              <span class="fname">{baseName(e.path)}</span>
            </button>
          {/each}
        </section>
      {/if}

      {#if !entries.length}
        <p class="empty">Working tree clean.</p>
      {/if}

      {#if selected}
        <section class="diff">
          <h3 class="difftitle">{baseName(selected.path)}</h3>
          {#if isSplit}
            <div class="diffbody split">
              {#each splitRows as row, index (index)}
                {#if row.hunk}
                  <div class="hunk">{row.hunkText}</div>
                {:else}
                  <div class="cell" class:filled-del={row.leftFilled}><ColorText text={row.left} /></div>
                  <div class="cell right" class:filled-add={row.rightFilled}><ColorText text={row.right} /></div>
                {/if}
              {/each}
            </div>
          {:else}
            <pre class="diffbody">{#each diffLines as line, index (index)}<span
              class="dl"
              class:add={line.kind === DiffKind.add}
              class:del={line.kind === DiffKind.del}
              class:meta={line.kind === DiffKind.meta}
            ><ColorText text={line.text} />
</span>{/each}</pre>
          {/if}
        </section>
      {/if}

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

  .restore {
    display: flex;
    flex-direction: column;
    gap: 9px;
    padding: 12px;
    border: 1px solid var(--outline);
    border-radius: var(--radius-medium);
    background: var(--surface-1);

    .eyebrow {
      display: flex;
      gap: 7px;
      align-items: center;
      margin: 0;
      color: var(--on-surface-variant);
      font-weight: 700;
      font-size: 11px;
      letter-spacing: 0.05em;
      text-transform: uppercase;
    }

    .lead {
      display: inline-flex;
      color: var(--primary);
    }

    .restore-input {
      display: flex;
      gap: 8px;
    }

    input {
      flex: 1;
      min-inline-size: 0;
      padding-block: 8px;
      padding-inline: 14px;
      border: 1px solid var(--outline);
      border-radius: 999px;
      background: var(--surface-2);
      color: var(--on-surface);
      font: inherit;
      font-size: 13px;

      &:focus-visible {
        border-color: var(--primary);
        outline: none;
      }
    }

    .go {
      flex: none;
      padding-block: 8px;
      padding-inline: 16px;
      border: none;
      border-radius: 999px;
      background: var(--primary);
      color: var(--on-primary);
      font-weight: 700;
      font-size: 13px;
      cursor: pointer;
      transition: filter 120ms var(--ease);

      &:hover:not(:disabled) {
        filter: brightness(1.06);
      }

      &:disabled {
        opacity: 60%;
        cursor: default;
      }
    }

    .hint {
      margin: 0;
      color: var(--on-surface-variant);
      font-size: 11px;
    }

    .hint code {
      font-family: var(--font-monospace);
    }

    .restore-msg {
      margin: 0;
      font-size: 12px;
    }

    .restore-msg code {
      font-family: var(--font-monospace);
      font-weight: 600;
    }

    .restore-msg.ok {
      color: var(--tertiary);
    }

    .restore-msg.crit {
      color: var(--critical);
      white-space: pre-wrap;
    }
  }

  .candidates {
    display: flex;
    flex-direction: column;
    gap: 4px;
    margin: 0;
    padding: 0;
    list-style: none;
  }

  .candidate {
    display: flex;
    flex-direction: column;
    gap: 4px;
    inline-size: 100%;
    padding-block: 8px;
    padding-inline: 10px;
    border: 1px solid var(--outline);
    border-radius: var(--radius-small);
    background: var(--surface-2);
    text-align: start;
    cursor: pointer;
    transition: border-color 120ms var(--ease);
    animation: line-in 240ms var(--ease) both;

    &:hover {
      border-color: var(--primary);
    }

    .cand-top {
      display: flex;
      gap: 10px;
      align-items: baseline;
    }

    .summary {
      overflow: hidden;
      font-size: 13px;
      text-overflow: ellipsis;
      white-space: nowrap;
    }

    .cand-bot {
      display: flex;
      gap: 10px;
      justify-content: space-between;
      align-items: center;
    }

    .conf {
      display: flex;
      flex: none;
      gap: 6px;
      align-items: center;
    }

    .bar {
      display: block;
      overflow: hidden;
      block-size: 4px;
      inline-size: 48px;
      border-radius: 999px;
      background: var(--surface-3);
    }

    .fill {
      display: block;
      block-size: 100%;
      border-radius: 999px;
      background: var(--tertiary);
    }

    .pct {
      color: var(--on-surface-variant);
      font-size: 11px;
      font-variant-numeric: tabular-nums;
    }
  }

  .n {
    padding-block: 1px;
    padding-inline: 7px;
    border-radius: 999px;
    background: var(--surface-2);
    color: var(--on-surface-variant);
    font-weight: 700;
    font-size: 11px;
    font-variant-numeric: tabular-nums;
  }

  .dot {
    block-size: 8px;
    inline-size: 8px;
    border-radius: 50%;
  }

  .dot.agent {
    background: var(--tertiary);
  }

  .dot.staged {
    background: var(--primary);
  }

  .row {
    display: flex;
    gap: 10px;
    align-items: center;
    inline-size: 100%;
    padding-block: 7px;
    padding-inline: 9px;
    border: none;
    border-radius: var(--radius-small);
    background: transparent;
    color: var(--on-surface);
    text-align: start;
    cursor: pointer;
  }

  .row:hover {
    background: var(--surface-2);
  }

  .row.sel {
    background: var(--primary-container);
    color: var(--on-primary-container);
  }

  .k {
    display: grid;
    flex: none;
    place-items: center;
    block-size: 18px;
    inline-size: 18px;
    border-radius: var(--radius-small);
    color: #ffffff;
    font-weight: 700;
    font-size: 11px;
  }

  .k.created,
  .k.untracked {
    background: var(--tertiary);
  }

  .k.modified,
  .k.renamed {
    background: var(--primary);
  }

  .k.deleted {
    background: var(--critical);
  }

  .fname {
    overflow: hidden;
    font-family: var(--font-monospace);
    font-size: 12px;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .diff {
    overflow: hidden;
    border: 1px solid var(--outline);
    border-radius: var(--radius-medium);
  }

  .difftitle {
    margin: 0;
    padding: 8px 12px;
    background: var(--surface-2);
    color: var(--on-surface);
    font-family: var(--font-monospace);
    font-weight: 600;
    font-size: 12px;
    letter-spacing: 0;
    text-transform: none;
  }

  .diffbody {
    overflow: auto;
    max-block-size: 300px;
    margin: 0;
    padding: 8px 0;
    background: var(--code-background);
    font-family: var(--font-monospace);
    font-size: 12px;
    line-height: 1.5;
  }

  .dl {
    display: block;
    padding-inline: 12px;
    color: var(--code-foreground);
    white-space: pre-wrap;
  }

  .dl.add {
    background: var(--tertiary-wash);
  }

  .dl.del {
    background: var(--critical-wash);
  }

  .dl.meta {
    color: var(--on-surface-variant);
  }

  /* Split (2-col) view — rows come from the shared `toSplitRows` (DRY). */
  .diffbody.split {
    display: grid;
    grid-template-columns: 1fr 1fr;

    .hunk {
      grid-column: 1 / -1;
      padding-inline: 12px;
      color: var(--on-surface-variant);
      white-space: pre;
    }

    .cell {
      overflow: hidden;
      min-block-size: 1.5em;
      padding-inline: 10px;
      border-inline-end: 1px solid var(--outline);
      color: var(--code-foreground);
      white-space: pre;
    }

    .cell.right {
      border-inline-end: none;
    }

    .cell.filled-del {
      background: var(--critical-wash);
    }

    .cell.filled-add {
      background: var(--tertiary-wash);
    }
  }

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
