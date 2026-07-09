<script lang="ts">
  import { onMount, onDestroy } from "svelte";
  import type { UnlistenFn } from "@tauri-apps/api/event";
  import { feed } from "./bridge";
  import type { ChangeEvent } from "./types";

  // Newest first. Capped so a busy agent session can't grow this unbounded.
  let events = $state<ChangeEvent[]>([]);
  const CAP = 300;
  let unlisten: UnlistenFn | undefined;

  onMount(async () => {
    unlisten = await feed.onChange((ev) => {
      events = [ev, ...events].slice(0, CAP);
    });
    // Watch the project the ADE was opened on.
    await feed.start();
  });

  onDestroy(() => unlisten?.());

  function fileName(p: string) {
    return p.split(/[\\/]/).pop() ?? p;
  }
  function dir(p: string) {
    const parts = p.split(/[\\/]/);
    parts.pop();
    return parts.join("/");
  }
  function ago(ts: number) {
    const s = Math.max(0, Math.round((Date.now() - ts) / 1000));
    if (s < 60) return `${s}s ago`;
    if (s < 3600) return `${Math.round(s / 60)}m ago`;
    return `${Math.round(s / 3600)}h ago`;
  }
</script>

<div class="feed">
  <header class="head">
    <h2>Change Feed</h2>
    <span class="count">{events.length}</span>
  </header>

  {#if events.length === 0}
    <p class="empty">
      Waiting for edits. Ask the agent to change a file and it appears here —
      what changed, and how much.
    </p>
  {/if}

  <ul class="cards">
    {#each events as ev (ev.id)}
      <li class="card {ev.kind}">
        <div class="row">
          <span class="dot {ev.kind}" aria-hidden="true"></span>
          <span class="name" title={ev.path}>{fileName(ev.path)}</span>
          <span class="time">{ago(ev.ts)}</span>
        </div>
        <p class="summary">{ev.summary}</p>
        <div class="meta">
          <span class="path">{dir(ev.path)}</span>
          <span class="stat">
            {#if ev.added}<span class="add">+{ev.added}</span>{/if}
            {#if ev.removed}<span class="del">−{ev.removed}</span>{/if}
          </span>
        </div>
      </li>
    {/each}
  </ul>
</div>

<style>
  .feed { height: 100%; display: flex; flex-direction: column; }
  .head {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 12px 16px;
    border-bottom: 1px solid var(--outline);
  }
  .head h2 { margin: 0; font-size: 15px; }
  .count {
    font-size: 12px;
    font-weight: 700;
    color: var(--on-primary-container);
    background: var(--primary-container);
    padding: 2px 9px;
    border-radius: 999px;
  }
  .empty {
    margin: 16px;
    font-size: 13px;
    color: var(--on-surface-var);
    line-height: 1.5;
  }
  .cards {
    list-style: none;
    margin: 0;
    padding: 10px;
    display: flex;
    flex-direction: column;
    gap: 8px;
    overflow-y: auto;
  }
  .card {
    background: var(--surface-1);
    border-radius: var(--r-md);
    padding: 12px 14px;
    border-left: 3px solid var(--outline);
    animation: rise 0.25s var(--ease);
  }
  .card.created { border-left-color: var(--tertiary); }
  .card.modified { border-left-color: var(--primary); }
  .card.deleted { border-left-color: var(--crit); }
  @keyframes rise {
    from { opacity: 0; transform: translateY(-4px); }
    to { opacity: 1; transform: none; }
  }
  .row { display: flex; align-items: center; gap: 8px; }
  .dot { width: 8px; height: 8px; border-radius: 50%; flex: none; }
  .dot.created { background: var(--tertiary); }
  .dot.modified { background: var(--primary); }
  .dot.deleted { background: var(--crit); }
  .name {
    font-family: var(--font-mono);
    font-size: 13px;
    font-weight: 600;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .time { margin-left: auto; font-size: 11px; color: var(--on-surface-var); }
  .summary { margin: 6px 0 8px; font-size: 13px; color: var(--on-surface); }
  .meta {
    display: flex;
    justify-content: space-between;
    align-items: center;
    gap: 8px;
  }
  .path {
    font-family: var(--font-mono);
    font-size: 11px;
    color: var(--on-surface-var);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .stat { font-family: var(--font-mono); font-size: 12px; display: flex; gap: 6px; }
  .add { color: var(--tertiary); }
  .del { color: var(--crit); }
</style>
