<script lang="ts">
  import { feed } from "../lib/bridge";
  import type { ChangeEvent } from "../lib/types";
  import type { UnlistenFn } from "@tauri-apps/api/event";
  import { onDestroy, onMount } from "svelte";

  // Newest first. Capped so a busy agent session can't grow this unbounded.
  let events = $state<ChangeEvent[]>([]);
  const CAP = 300;
  let unlisten: UnlistenFn | undefined;

  onMount(async () => {
    unlisten = await feed.onChange(event => {
      events = [event, ...events].slice(0, CAP);
    });
    // Watch the project the ADE was opened on.
    await feed.start();
  });

  onDestroy(() => unlisten?.());

  function fileName(path: string) {
    return path.split(/[\\/]/).pop() ?? path;
  }
  function dir(path: string) {
    const parts = path.split(/[\\/]/);
    parts.pop();
    return parts.join("/");
  }
  function ago(stamp: number) {
    const secs = Math.max(0, Math.round((Date.now() - stamp) / 1000));
    if (secs < 60) {
      return `${secs}s ago`;
    }

    if (secs < 3600) {
      return `${Math.round(secs / 60)}m ago`;
    }

    return `${Math.round(secs / 3600)}h ago`;
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
            {#if ev.added}
              <span class="add">+{ev.added}</span>
            {/if}
            {#if ev.removed}
              <span class="del">−{ev.removed}</span>
            {/if}
          </span>
        </div>
      </li>
    {/each}
  </ul>
</div>

<style>
  .feed {
    display: flex;
    flex-direction: column;
    height: 100%;
  }

  .head {
    display: flex;
    gap: 8px;
    align-items: center;
    padding: 12px 16px;
    border-bottom: 1px solid var(--outline);
  }

  .head h2 {
    margin: 0;
    font-size: 15px;
  }

  .count {
    padding: 2px 9px;
    border-radius: 999px;
    background: var(--primary-container);
    color: var(--on-primary-container);
    font-weight: 700;
    font-size: 12px;
  }

  .empty {
    margin: 16px;
    color: var(--on-surface-var);
    font-size: 13px;
    line-height: 1.5;
  }

  .cards {
    display: flex;
    flex-direction: column;
    gap: 8px;
    overflow-y: auto;
    margin: 0;
    padding: 10px;
    list-style: none;
  }

  .card {
    padding: 12px 14px;
    border-left: 3px solid var(--outline);
    border-radius: var(--r-md);
    background: var(--surface-1);
    animation: rise 250ms var(--ease);
  }

  .card.created {
    border-left-color: var(--tertiary);
  }

  .card.modified {
    border-left-color: var(--primary);
  }

  .card.deleted {
    border-left-color: var(--crit);
  }

  @keyframes rise {
    from {
      opacity: 0%;
      transform: translateY(-4px);
    }

    to {
      opacity: 100%;
      transform: none;
    }
  }

  .row {
    display: flex;
    gap: 8px;
    align-items: center;
  }

  .dot {
    flex: none;
    width: 8px;
    height: 8px;
    border-radius: 50%;
  }

  .dot.created {
    background: var(--tertiary);
  }

  .dot.modified {
    background: var(--primary);
  }

  .dot.deleted {
    background: var(--crit);
  }

  .name {
    overflow: hidden;
    font-family: var(--font-mono);
    font-weight: 600;
    font-size: 13px;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .time {
    margin-left: auto;
    color: var(--on-surface-var);
    font-size: 11px;
  }

  .summary {
    margin-block: 6px 8px;
    margin-inline: 0;
    color: var(--on-surface);
    font-size: 13px;
  }

  .meta {
    display: flex;
    gap: 8px;
    justify-content: space-between;
    align-items: center;
  }

  .path {
    overflow: hidden;
    color: var(--on-surface-var);
    font-family: var(--font-mono);
    font-size: 11px;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .stat {
    display: flex;
    gap: 6px;
    font-family: var(--font-mono);
    font-size: 12px;
  }

  .add {
    color: var(--tertiary);
  }

  .del {
    color: var(--crit);
  }
</style>
