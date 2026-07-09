<script lang="ts">
  import { onMount, onDestroy } from "svelte";
  import type { UnlistenFn } from "@tauri-apps/api/event";
  import { vcs, feed } from "../lib/bridge";
  import type { Commit, StatusEntry } from "../lib/types";

  let entries = $state<StatusEntry[]>([]);
  let commits = $state<Commit[]>([]);
  let error = $state<string | null>(null);
  let selected = $state<StatusEntry | null>(null);
  let diff = $state("");
  let unlisten: UnlistenFn | undefined;

  // Grouped views — agent-oriented review lives in the "unreviewed" (unstaged)
  // group; "approve" moves entries into the staged group.
  const unstaged = $derived(entries.filter((e) => !e.staged));
  const staged = $derived(entries.filter((e) => e.staged));

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
    diff = entry.kind === "untracked" ? "(new file — not yet tracked)" : await vcs.diff(entry.path, entry.staged);
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

  const fileName = (p: string) => p.split(/[\\/]/).pop() ?? p;
  const diffLines = $derived(diff ? diff.split("\n") : []);
  const lineClass = (l: string): "add" | "del" | "meta" | "" => {
    if (l.startsWith("+") && !l.startsWith("+++")) return "add";
    if (l.startsWith("-") && !l.startsWith("---")) return "del";
    if (l.startsWith("@@") || l.startsWith("diff ")) return "meta";
    return "";
  };
</script>

<div class="vcs">
  <header class="head">
    <h2>Version control</h2>
    <button class="refresh" onclick={refresh} title="Refresh">⟳</button>
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
          <pre class="diffbody">{#each diffLines as l}<span class="dl {lineClass(l)}">{l}
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
  .vcs { display: flex; flex-direction: column; height: 100%; }
  .head {
    display: flex;
    align-items: center;
    gap: 8px;
    padding-block: 12px;
    padding-inline: 16px;
    border-block-end: 1px solid var(--outline);
  }
  .head h2 { margin: 0; font-size: 15px; }
  .refresh {
    margin-inline-start: auto;
    font-size: 16px;
    line-height: 1;
    color: var(--on-surface-var);
    background: var(--surface-2);
    border: none;
    inline-size: 30px;
    block-size: 30px;
    border-radius: 999px;
    cursor: pointer;
  }
  .refresh:hover { color: var(--primary); }
  .scroll { overflow-y: auto; padding: 8px 10px; display: flex; flex-direction: column; gap: 14px; }
  .empty { margin: 16px; font-size: 13px; color: var(--on-surface-var); }

  .group h3 {
    display: flex;
    align-items: center;
    gap: 6px;
    margin: 4px 4px 8px;
    font-size: 12px;
    text-transform: uppercase;
    letter-spacing: 0.08em;
    color: var(--on-surface-var);
  }
  .n {
    font-size: 11px;
    background: var(--surface-3);
    color: var(--on-surface-var);
    padding-inline: 7px;
    border-radius: 999px;
  }
  .dot { inline-size: 8px; block-size: 8px; border-radius: 50%; }
  .dot.agent { background: var(--tertiary); }
  .dot.staged { background: var(--primary); }

  .row {
    display: flex;
    align-items: center;
    gap: 10px;
    inline-size: 100%;
    text-align: start;
    padding: 6px 8px;
    border: none;
    background: transparent;
    border-radius: var(--r-sm);
    cursor: pointer;
    color: var(--on-surface);
  }
  .row:hover { background: var(--surface-2); }
  .row.sel { background: var(--primary-container); color: var(--on-primary-container); }
  .k {
    inline-size: 18px;
    block-size: 18px;
    display: grid;
    place-items: center;
    border-radius: var(--r-sm);
    font-size: 11px;
    font-weight: 700;
    color: #fff;
    flex: none;
  }
  .k.created, .k.untracked { background: var(--tertiary); }
  .k.modified, .k.renamed { background: var(--primary); }
  .k.deleted { background: var(--crit); }
  .fname { font-family: var(--font-mono); font-size: 13px; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }

  .diff { border: 1px solid var(--outline); border-radius: var(--r-md); overflow: hidden; }
  .difftitle {
    margin: 0;
    padding: 8px 12px;
    font-family: var(--font-mono);
    font-size: 12px;
    background: var(--surface-2);
    color: var(--on-surface);
    text-transform: none;
    letter-spacing: 0;
  }
  .diffbody {
    margin: 0;
    padding: 8px 0;
    max-block-size: 320px;
    overflow: auto;
    background: var(--code-bg);
    font-family: var(--font-mono);
    font-size: 12px;
    line-height: 1.45;
  }
  .dl { display: block; padding-inline: 12px; color: var(--code-fg); white-space: pre-wrap; }
  .dl.add { background: color-mix(in srgb, var(--tertiary) 22%, transparent); }
  .dl.del { background: color-mix(in srgb, var(--crit) 22%, transparent); }
  .dl.meta { color: var(--on-surface-var); }

  .log .commit { padding: 6px 8px; display: grid; gap: 2px; }
  .sha { font-family: var(--font-mono); font-size: 11px; color: var(--primary); }
  .msg { font-size: 13px; }
  .by { font-size: 11px; color: var(--on-surface-var); }
</style>
