<script lang="ts">
  import Terminal from "./panels/Terminal.svelte";
  import ChangeFeed from "./panels/ChangeFeed.svelte";
  import { pty } from "./lib/bridge";

  // MVP layout: one calm surface — terminal on the left, a switchable side
  // panel on the right. Heavy/optional panels lazy-load (tree-shaking).
  type Side = "feed" | "vcs" | "config" | null;
  let side = $state<Side>("feed");

  const toggle = (p: Exclude<Side, null>) => (side = side === p ? null : p);

  // Highlight → agent bridge: text selected in a side panel can be sent
  // straight into the terminal's input (the xterm terminal owns its own
  // selection, so we only bridge selections made outside it).
  let selection = $state("");

  function readSelection() {
    const sel = window.getSelection();
    const text = sel?.toString().trim() ?? "";
    const inSidePanel = sel?.anchorNode instanceof Node &&
      document.querySelector(".side-pane")?.contains(sel.anchorNode);
    selection = text && inSidePanel ? text : "";
  }

  async function sendToAgent() {
    if (!selection) return;
    await pty.write(selection);
    selection = "";
    window.getSelection()?.removeAllRanges();
  }
</script>

<svelte:document onselectionchange={readSelection} />

<div class="shell">
  <header class="topbar">
    <span class="brand">◆ ADE</span>
    <span class="sub">agentic development environment</span>
    <div class="spacer"></div>
    <div class="seg" role="tablist" aria-label="Side panel">
      <button role="tab" aria-selected={side === "feed"} onclick={() => toggle("feed")}>
        Change Feed
      </button>
      <button role="tab" aria-selected={side === "vcs"} onclick={() => toggle("vcs")}>
        Git
      </button>
      <button role="tab" aria-selected={side === "config"} onclick={() => toggle("config")}>
        Config
      </button>
    </div>
  </header>

  <main class="body" class:with-side={side !== null}>
    <section class="pane term-pane">
      <Terminal />
    </section>

    {#if side !== null}
      <aside class="pane side-pane">
        {#if side === "feed"}
          <ChangeFeed />
        {:else if side === "vcs"}
          {#await import("./panels/VcsPanel.svelte") then { default: VcsPanel }}
            <VcsPanel />
          {/await}
        {:else if side === "config"}
          {#await import("./panels/ConfigPanel.svelte") then { default: ConfigPanel }}
            <ConfigPanel />
          {/await}
        {/if}
      </aside>
    {/if}
  </main>

  {#if selection}
    <button class="send-fab" onclick={sendToAgent}>
      ◆ Send to agent
      <span class="preview">{selection.length > 40 ? selection.slice(0, 40) + "…" : selection}</span>
    </button>
  {/if}
</div>

<style>
  .shell { display: flex; flex-direction: column; block-size: 100%; }
  .topbar {
    display: flex;
    align-items: center;
    gap: 10px;
    padding-block: 10px;
    padding-inline: 16px;
    background: var(--surface-1);
    border-block-end: 1px solid var(--outline);
  }
  .brand { font-weight: 700; color: var(--primary); letter-spacing: 0.02em; }
  .sub {
    font-size: 12px;
    color: var(--on-surface-var);
    text-transform: uppercase;
    letter-spacing: 0.12em;
  }
  .spacer { flex: 1; }

  .seg { display: inline-flex; background: var(--surface-2); border-radius: 999px; padding: 3px; }
  .seg button {
    font: inherit;
    font-size: 13px;
    font-weight: 600;
    color: var(--on-surface-var);
    background: transparent;
    border: none;
    padding: 6px 14px;
    border-radius: 999px;
    cursor: pointer;
    transition: background 0.2s var(--ease), color 0.2s var(--ease);
  }
  .seg button[aria-selected="true"] {
    background: var(--primary-container);
    color: var(--on-primary-container);
  }

  .body { flex: 1; display: grid; grid-template-columns: 1fr; min-block-size: 0; }
  .body.with-side { grid-template-columns: 1fr minmax(320px, 420px); }
  .pane { min-block-size: 0; min-inline-size: 0; overflow: hidden; }
  .side-pane { border-inline-start: 1px solid var(--outline); background: var(--surface); }

  @media (max-width: 720px) {
    .body.with-side { grid-template-columns: 1fr; grid-template-rows: 1fr 40%; }
  }

  .send-fab {
    position: fixed;
    inset-block-end: 20px;
    inset-inline-start: 50%;
    translate: -50% 0;
    display: inline-flex;
    align-items: center;
    gap: 10px;
    font: inherit;
    font-weight: 600;
    color: var(--on-primary);
    background: var(--primary);
    border: none;
    padding: 12px 20px;
    border-radius: 999px;
    box-shadow: 0 6px 20px color-mix(in srgb, var(--primary) 40%, transparent);
    cursor: pointer;
    animation: pop 0.2s var(--ease);
  }
  .send-fab .preview {
    font-family: var(--font-mono);
    font-size: 12px;
    font-weight: 400;
    opacity: 0.85;
    max-inline-size: 40ch;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  @keyframes pop {
    from { opacity: 0; translate: -50% 8px; }
    to { opacity: 1; translate: -50% 0; }
  }
</style>
