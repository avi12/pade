<script lang="ts">
  import { config } from "../lib/bridge";
  import type { ConfigFile } from "../lib/types";
  import { onMount } from "svelte";

  // Only the config files relevant to the active agent are listed.
  const { agent }: { agent: string } = $props();

  let files = $state<ConfigFile[]>([]);
  let selected = $state<ConfigFile | null>(null);
  let content = $state("");

  async function open(file: ConfigFile) {
    if (!file.exists) {
      return;
    }

    selected = file;
    content = await config.read(file.rel);
  }

  onMount(async () => {
    files = await config.list(agent);
    const first = files.find(file => file.exists);
    if (first) {
      await open(first);
    }
  });
</script>

<div class="cfg">
  <header class="head"><h2>Agent config</h2></header>

  <div class="scroll">
    <ul class="list">
      {#each files as f (f.rel)}
        <li>
          <button
            class="row"
            class:sel={selected?.rel === f.rel}
            disabled={!f.exists}
            onclick={() => open(f)}
          >
            <span class="kind {f.kind}">{f.kind}</span>
            <span class="rel">{f.rel}</span>
            {#if !f.exists}
              <span class="missing">absent</span>
            {/if}
          </button>
        </li>
      {/each}
    </ul>

    {#if selected}
      <section class="viewer">
        <div class="card">
          <h3>{selected.rel}</h3>
          <pre class="body">{content}</pre>
        </div>
        <p class="note">Read-only in the MVP — edits will write back to this same file.</p>
      </section>
    {/if}
  </div>
</div>

<style>
  .cfg {
    display: flex;
    flex-direction: column;
    block-size: 100%;
  }

  .head {
    padding-block: 12px;
    padding-inline: 16px;
    border-block-end: 1px solid var(--outline);
  }

  .head h2 {
    margin: 0;
    font-size: 15px;
  }

  .scroll {
    display: flex;
    flex-direction: column;
    gap: 10px;
    overflow-y: auto;
    padding: 10px;
    animation: panel-swap 280ms var(--ease);
  }

  .list {
    display: flex;
    flex-direction: column;
    gap: 2px;
    margin: 0;
    padding: 0;
    list-style: none;
  }

  .row {
    display: flex;
    gap: 10px;
    align-items: center;
    inline-size: 100%;
    padding-block: 8px;
    padding-inline: 10px;
    border: none;
    border-radius: var(--r-sm);
    background: transparent;
    color: var(--on-surface);
    text-align: start;
    cursor: pointer;
    transition: background 140ms var(--ease);
  }

  .row:hover:not(:disabled) {
    background: var(--surface-2);
  }

  .row:disabled {
    opacity: 45%;
    cursor: default;
  }

  .row.sel {
    background: var(--primary-container);
    color: var(--on-primary-container);
  }

  .kind {
    flex: none;
    padding-block: 2px;
    padding-inline: 9px;
    border-radius: 999px;
    background: var(--surface-3);
    color: var(--on-surface-var);
    font-weight: 700;
    font-size: 10px;
    letter-spacing: 0.05em;
    text-transform: uppercase;
  }

  .kind.instructions {
    background: var(--tertiary-wash);
    color: var(--tertiary);
  }

  .kind.mcp {
    background: var(--primary-container);
    color: var(--on-primary-container);
  }

  .rel {
    overflow: hidden;
    font-family: var(--font-mono);
    font-size: 12px;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .missing {
    margin-inline-start: auto;
    color: var(--on-surface-var);
    font-style: italic;
    font-size: 10px;
  }

  .viewer {
    display: flex;
    flex-direction: column;
    gap: 2px;
    margin-block-start: 6px;
  }

  .card {
    overflow: hidden;
    border: 1px solid var(--outline);
    border-radius: var(--r-md);
  }

  .viewer h3 {
    margin: 0;
    padding-block: 8px;
    padding-inline: 12px;
    background: var(--surface-2);
    color: var(--on-surface);
    font-family: var(--font-mono);
    font-weight: 600;
    font-size: 12px;
  }

  .body {
    overflow: auto;
    max-block-size: 280px;
    margin: 0;
    padding: 12px;
    background: var(--code-bg);
    color: var(--code-fg);
    font-family: var(--font-mono);
    font-size: 12px;
    line-height: 1.55;
    white-space: pre-wrap;
  }

  .note {
    margin-block: 2px 0;
    margin-inline: 0;
    color: var(--on-surface-var);
    font-style: italic;
    font-size: 11px;
  }
</style>
