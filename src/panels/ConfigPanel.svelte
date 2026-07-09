<script lang="ts">
  import { onMount } from "svelte";
  import { config } from "../lib/bridge";
  import type { ConfigFile } from "../lib/types";

  let files = $state<ConfigFile[]>([]);
  let selected = $state<ConfigFile | null>(null);
  let content = $state("");

  async function open(f: ConfigFile) {
    if (!f.exists) return;
    selected = f;
    content = await config.read(f.rel);
  }

  onMount(async () => {
    files = await config.list();
    const first = files.find((f) => f.exists);
    if (first) await open(first);
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
            {#if !f.exists}<span class="missing">absent</span>{/if}
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
  .cfg { display: flex; flex-direction: column; block-size: 100%; }
  .head {
    padding-block: 12px;
    padding-inline: 16px;
    border-block-end: 1px solid var(--outline);
  }
  .head h2 { margin: 0; font-size: 15px; }
  .scroll { overflow-y: auto; padding: 10px; display: flex; flex-direction: column; gap: 12px; }

  .list { list-style: none; margin: 0; padding: 0; display: flex; flex-direction: column; gap: 2px; }
  .row {
    display: flex;
    align-items: center;
    gap: 10px;
    inline-size: 100%;
    text-align: start;
    padding: 7px 8px;
    border: none;
    background: transparent;
    border-radius: var(--r-sm);
    cursor: pointer;
    color: var(--on-surface);
  }
  .row:hover:not(:disabled) { background: var(--surface-2); }
  .row:disabled { opacity: 0.45; cursor: default; }
  .row.sel { background: var(--primary-container); color: var(--on-primary-container); }
  .kind {
    font-size: 10px;
    font-weight: 700;
    text-transform: uppercase;
    letter-spacing: 0.06em;
    padding: 2px 8px;
    border-radius: 999px;
    background: var(--surface-3);
    color: var(--on-surface-var);
    flex: none;
  }
  .kind.instructions { background: color-mix(in srgb, var(--tertiary) 25%, transparent); }
  .kind.mcp { background: color-mix(in srgb, var(--primary) 22%, transparent); }
  .rel { font-family: var(--font-mono); font-size: 13px; }
  .missing { margin-inline-start: auto; font-size: 11px; color: var(--on-surface-var); }

  .viewer h3 {
    margin: 0 0 6px;
    font-family: var(--font-mono);
    font-size: 12px;
    color: var(--on-surface-var);
  }
  .body {
    margin: 0;
    padding: 12px;
    max-block-size: 380px;
    overflow: auto;
    background: var(--code-bg);
    color: var(--code-fg);
    font-family: var(--font-mono);
    font-size: 12px;
    line-height: 1.5;
    border-radius: var(--r-md);
    white-space: pre-wrap;
  }
  .note { margin: 8px 2px 0; font-size: 12px; color: var(--on-surface-var); }
</style>
