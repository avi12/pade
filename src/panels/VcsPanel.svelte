<script lang="ts">
  import { feed, vcs } from "@/lib/bridge";
  import { DiffKind, parseDiff } from "@/lib/diff";
  import Icon from "@/lib/Icon.svelte";
  import { VcsKind } from "@/lib/types";
  import type { Commit, RestoreCandidate, StatusEntry } from "@/lib/types";
  import type { UnlistenFn } from "@tauri-apps/api/event";
  import { onDestroy, onMount } from "svelte";

  let entries = $state<StatusEntry[]>([]);
  let commits = $state<Commit[]>([]);
  let error = $state<string | null>(null);
  let selected = $state<StatusEntry | null>(null);
  let diff = $state("");
  let unlisten: UnlistenFn | undefined;

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
    const query = restoreQuery.trim();
    const isEmptyQuery = query.length === 0;
    if (isEmptyQuery) {
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
    unlisten = await feed.onChange(scheduleRefresh);
  });
  onDestroy(() => {
    unlisten?.();
    clearTimeout(timer);
  });

  function fileName(path: string) {
    return path.split(/[\\/]/).pop() ?? path;
  }

  // Diff rendering funnels through the shared parser (DRY) — one authoritative
  // line classifier for both the Change Feed and this panel.
  const diffLines = $derived(parseDiff(diff));

  // Confidence as a 0..100 percentage — scores run 0..≈1.5, clamped for display.
  function confidencePct(score: number): number {
    return Math.round(Math.min(score / 1.5, 1) * 100);
  }
</script>

<div class="vcs">
  <header class="head">
    <h2>Version control</h2>
    <button class="refresh" aria-label="Refresh" data-tooltip="Refresh" onclick={refresh}>⟳</button>
  </header>

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
                      <span class="pct">{confidencePct(c.score)}%</span>
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
          <h3><span class="dot agent"></span> Unreviewed <span class="n">{unstaged.length}</span></h3>
          {#each unstaged as e (e.path)}
            <button class="row" class:sel={selected?.path === e.path} onclick={() => open(e)}>
              <span class="k {e.kind}">{e.kind[0].toUpperCase()}</span>
              <span class="fname">{fileName(e.path)}</span>
            </button>
          {/each}
        </section>
      {/if}

      {#if staged.length}
        <section class="group">
          <h3><span class="dot staged"></span> Staged <span class="n">{staged.length}</span></h3>
          {#each staged as e (e.path)}
            <button class="row" class:sel={selected?.path === e.path} onclick={() => open(e)}>
              <span class="k {e.kind}">{e.kind[0].toUpperCase()}</span>
              <span class="fname">{fileName(e.path)}</span>
            </button>
          {/each}
        </section>
      {/if}

      {#if !entries.length}
        <p class="empty">Working tree clean.</p>
      {/if}

      {#if selected}
        <section class="diff">
          <h3 class="difftitle">{fileName(selected.path)}</h3>
          <pre class="diffbody">{#each diffLines as line, index (index)}<span
            class="dl"
            class:add={line.kind === DiffKind.add}
            class:del={line.kind === DiffKind.del}
            class:meta={line.kind === DiffKind.meta}
          >{line.text}
</span>{/each}</pre>
        </section>
      {/if}

      <section class="group log">
        <h3>Recent commits</h3>
        {#each commits as c (c.id)}
          <div class="commit">
            <code class="sha">{c.short}</code>
            <span class="msg">{c.summary}</span>
            <span class="by">{c.author} · {c.when}</span>
          </div>
        {/each}
      </section>
    </div>
  {/if}
</div>

<style>
  .vcs {
    display: flex;
    flex-direction: column;
    height: 100%;
  }

  .head {
    display: flex;
    gap: 8px;
    align-items: center;
    padding-block: 12px;
    padding-inline: 16px;
    border-block-end: 1px solid var(--outline);
  }

  .head h2 {
    margin: 0;
    font-size: 15px;
  }

  .refresh {
    block-size: 30px;
    inline-size: 30px;
    margin-inline-start: auto;
    border: none;
    border-radius: 999px;
    background: var(--surface-2);
    color: var(--on-surface-var);
    font-size: 16px;
    line-height: 1;
    cursor: pointer;
  }

  .refresh:hover {
    color: var(--primary);
  }

  .scroll {
    display: flex;
    flex-direction: column;
    gap: 14px;
    overflow-y: auto;
    padding: 8px 10px;
  }

  .empty {
    margin: 16px;
    color: var(--on-surface-var);
    font-size: 13px;
  }

  .restore {
    display: flex;
    flex-direction: column;
    gap: 9px;
    padding: 12px;
    border: 1px solid var(--outline);
    border-radius: var(--r-md);
    background: var(--surface-1);

    .eyebrow {
      display: flex;
      gap: 7px;
      align-items: center;
      margin: 0;
      color: var(--on-surface-var);
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
      color: var(--on-surface-var);
      font-size: 11px;
    }

    .hint code {
      font-family: var(--font-mono);
    }

    .restore-msg {
      margin: 0;
      font-size: 12px;
    }

    .restore-msg code {
      font-family: var(--font-mono);
      font-weight: 600;
    }

    .restore-msg.ok {
      color: var(--tertiary);
    }

    .restore-msg.crit {
      color: var(--crit);
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
    border-radius: var(--r-sm);
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
      color: var(--on-surface-var);
      font-size: 11px;
      font-variant-numeric: tabular-nums;
    }
  }

  .group h3 {
    display: flex;
    gap: 8px;
    align-items: center;
    margin-block: 4px 8px;
    margin-inline: 4px;
    color: var(--on-surface-var);
    font-weight: 700;
    font-size: 11px;
    letter-spacing: 0.06em;
    text-transform: uppercase;
  }

  .n {
    padding-block: 1px;
    padding-inline: 7px;
    border-radius: 999px;
    background: var(--surface-2);
    color: var(--on-surface-var);
    font-weight: 700;
    font-size: 11px;
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
    border-radius: var(--r-sm);
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
    border-radius: var(--r-sm);
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
    background: var(--crit);
  }

  .fname {
    overflow: hidden;
    font-family: var(--font-mono);
    font-size: 12px;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .diff {
    overflow: hidden;
    border: 1px solid var(--outline);
    border-radius: var(--r-md);
  }

  .difftitle {
    margin: 0;
    padding: 8px 12px;
    background: var(--surface-2);
    color: var(--on-surface);
    font-family: var(--font-mono);
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
    background: var(--code-bg);
    font-family: var(--font-mono);
    font-size: 12px;
    line-height: 1.5;
  }

  .dl {
    display: block;
    padding-inline: 12px;
    color: var(--code-fg);
    white-space: pre-wrap;
  }

  .dl.add {
    background: var(--tertiary-wash);
  }

  .dl.del {
    background: var(--crit-wash);
  }

  .dl.meta {
    color: var(--on-surface-var);
  }

  .log .commit {
    display: grid;
    grid-template-columns: auto 1fr;
    gap: 2px 10px;
    align-items: baseline;
    padding-block: 4px;
    padding-inline: 2px;
  }

  .sha {
    color: var(--primary);
    font-family: var(--font-mono);
    font-weight: 600;
    font-size: 12px;
  }

  .msg {
    font-size: 13px;
  }

  .by {
    grid-column: 1 / -1;
    color: var(--on-surface-var);
    font-size: 11px;
  }

  .candidate .by {
    grid-column: auto;
  }
</style>
