<script lang="ts">
  import { feed, vcs } from "../lib/bridge";
  import { VcsKind } from "../lib/types";
  import type { Commit, StatusEntry } from "../lib/types";
  import type { UnlistenFn } from "@tauri-apps/api/event";
  import { onDestroy, onMount } from "svelte";

  let entries = $state<StatusEntry[]>([]);
  let commits = $state<Commit[]>([]);
  let error = $state<string | null>(null);
  let selected = $state<StatusEntry | null>(null);
  let diff = $state("");
  let unlisten: UnlistenFn | undefined;

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
  const diffLines = $derived(diff ? diff.split("\n") : []);
  function lineClass(line: string): "add" | "del" | "meta" | "" {
    if (line.startsWith("+") && !line.startsWith("+++")) {
      return "add";
    }

    if (line.startsWith("-") && !line.startsWith("---")) {
      return "del";
    }

    if (line.startsWith("@@") || line.startsWith("diff ")) {
      return "meta";
    }

    return "";
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
          <pre class="diffbody">{#each diffLines as line, index (index)}<span class="dl {lineClass(line)}">{line}
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

  .group h3 {
    display: flex;
    gap: 6px;
    align-items: center;
    margin-block: 4px 8px;
    margin-inline: 4px;
    color: var(--on-surface-var);
    font-size: 12px;
    letter-spacing: 0.08em;
    text-transform: uppercase;
  }

  .n {
    padding-inline: 7px;
    border-radius: 999px;
    background: var(--surface-3);
    color: var(--on-surface-var);
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
    padding: 6px 8px;
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
    font-size: 13px;
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
    font-size: 12px;
    letter-spacing: 0;
    text-transform: none;
  }

  .diffbody {
    overflow: auto;
    max-block-size: 320px;
    margin: 0;
    padding: 8px 0;
    background: var(--code-bg);
    font-family: var(--font-mono);
    font-size: 12px;
    line-height: 1.45;
  }

  .dl {
    display: block;
    padding-inline: 12px;
    color: var(--code-fg);
    white-space: pre-wrap;
  }

  .dl.add {
    background: color-mix(in sRGB, var(--tertiary) 22%, transparent);
  }

  .dl.del {
    background: color-mix(in sRGB, var(--crit) 22%, transparent);
  }

  .dl.meta {
    color: var(--on-surface-var);
  }

  .log .commit {
    display: grid;
    gap: 2px;
    padding: 6px 8px;
  }

  .sha {
    color: var(--primary);
    font-family: var(--font-mono);
    font-size: 11px;
  }

  .msg {
    font-size: 13px;
  }

  .by {
    color: var(--on-surface-var);
    font-size: 11px;
  }
</style>
