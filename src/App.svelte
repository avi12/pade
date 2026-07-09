<script lang="ts">
  import Terminal from "./lib/Terminal.svelte";
  import ChangeFeed from "./lib/ChangeFeed.svelte";

  // MVP layout: one calm surface — terminal on the left, Change Feed on the
  // right. Everything else in the vision is a summon-able panel added later.
  let feedOpen = $state(true);
</script>

<div class="shell">
  <header class="topbar">
    <span class="brand">◆ ADE</span>
    <span class="sub">agentic development environment</span>
    <div class="spacer"></div>
    <button
      class="toggle"
      aria-pressed={feedOpen}
      onclick={() => (feedOpen = !feedOpen)}
    >
      {feedOpen ? "Hide" : "Show"} Change Feed
    </button>
  </header>

  <main class="body" class:with-feed={feedOpen}>
    <section class="pane term-pane">
      <Terminal />
    </section>
    {#if feedOpen}
      <aside class="pane feed-pane">
        <ChangeFeed />
      </aside>
    {/if}
  </main>
</div>

<style>
  .shell {
    display: flex;
    flex-direction: column;
    height: 100%;
  }
  .topbar {
    display: flex;
    align-items: center;
    gap: 10px;
    padding: 10px 16px;
    background: var(--surface-1);
    border-bottom: 1px solid var(--outline);
  }
  .brand {
    font-weight: 700;
    color: var(--primary);
    letter-spacing: 0.02em;
  }
  .sub {
    font-size: 12px;
    color: var(--on-surface-var);
    text-transform: uppercase;
    letter-spacing: 0.12em;
  }
  .spacer { flex: 1; }
  .toggle {
    font-family: var(--font-ui);
    font-size: 13px;
    font-weight: 600;
    color: var(--on-primary-container);
    background: var(--primary-container);
    border: none;
    padding: 8px 16px;
    border-radius: 999px;
    cursor: pointer;
    transition: filter 0.2s var(--ease);
  }
  .toggle:hover { filter: brightness(1.05); }

  .body {
    flex: 1;
    display: grid;
    grid-template-columns: 1fr;
    min-height: 0;
  }
  .body.with-feed {
    grid-template-columns: 1fr minmax(320px, 420px);
  }
  .pane { min-height: 0; min-width: 0; overflow: hidden; }
  .feed-pane {
    border-left: 1px solid var(--outline);
    background: var(--surface);
  }

  @media (max-width: 720px) {
    .body.with-feed { grid-template-columns: 1fr; grid-template-rows: 1fr 40%; }
  }
</style>
