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
        <h3>{selected.rel}</h3>
        <pre class="body">{content}</pre>
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
    gap: 12px;
    overflow-y: auto;
    padding: 10px;
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
    padding: 7px 8px;
    border: none;
    border-radius: var(--r-sm);
    background: transparent;
    color: var(--on-surface);
    text-align: start;
    cursor: pointer;
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
    padding: 2px 8px;
    border-radius: 999px;
    background: var(--surface-3);
    color: var(--on-surface-var);
    font-weight: 700;
    font-size: 10px;
    letter-spacing: 0.06em;
    text-transform: uppercase;
  }

  .kind.instructions {
    background: color-mix(in sRGB, var(--tertiary) 25%, transparent);
  }

  .kind.mcp {
    background: color-mix(in sRGB, var(--primary) 22%, transparent);
  }

  .rel {
    font-family: var(--font-mono);
    font-size: 13px;
  }

  .missing {
    margin-inline-start: auto;
    color: var(--on-surface-var);
    font-size: 11px;
  }

  .viewer h3 {
    margin-block: 0 6px;
    margin-inline: 0;
    color: var(--on-surface-var);
    font-family: var(--font-mono);
    font-size: 12px;
  }

  .body {
    overflow: auto;
    max-block-size: 380px;
    margin: 0;
    padding: 12px;
    border-radius: var(--r-md);
    background: var(--code-bg);
    color: var(--code-fg);
    font-family: var(--font-mono);
    font-size: 12px;
    line-height: 1.5;
    white-space: pre-wrap;
  }

  .note {
    margin-block: 8px 0;
    margin-inline: 2px;
    color: var(--on-surface-var);
    font-size: 12px;
  }
</style>
