<script lang="ts">
  import Terminal from "./panels/Terminal.svelte";
  import ChangeFeed from "./panels/ChangeFeed.svelte";

  // MVP layout: one calm surface — terminal on the left, a switchable side
  // panel on the right. Heavy/optional panels lazy-load (tree-shaking).
  type Side = "feed" | "vcs" | null;
  let side = $state<Side>("feed");

  const toggle = (p: Exclude<Side, null>) => (side = side === p ? null : p);
</script>

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
        {/if}
      </aside>
    {/if}
  </main>
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
</style>
