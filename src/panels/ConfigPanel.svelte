<script lang="ts">
  import { config } from "@/lib/bridge";
  import { collectVars } from "@/lib/colors";
  import ColorText from "@/lib/ColorText.svelte";
  import { setPanelHeader } from "@/lib/stores/sidePanel.svelte";
  import type { ConfigFile } from "@/lib/types";

  // Only the config files relevant to the active agent are listed.
  const { agent }: { agent: string } = $props();

  let files = $state<ConfigFile[]>([]);
  let selected = $state<ConfigFile | null>(null);
  let content = $state("");
  // Trace var(--x) swatches against the file's own token definitions first.
  const fileVars = $derived(collectVars(content));

  async function open(file: ConfigFile) {
    if (!file.exists) {
      return;
    }

    selected = file;
    // The read can resolve out of order relative to a later selection; only
    // apply it while this file is still the selected one.
    const requested = file;
    try {
      const text = await config.read(requested.rel);
      if (selected?.rel === requested.rel) {
        content = text;
      }
    } catch {
      if (selected?.rel === requested.rel) {
        content = "";
      }
    }
  }

  // `agent` is a reactive prop and the panel is not remounted when the active
  // agent changes, so reload the file list whenever it does. Capture the agent
  // to discard responses from a superseded agent.
  $effect(() => {
    const requestedAgent = agent;
    selected = null;
    content = "";
    files = [];
    (async () => {
      const listed = await config.list(requestedAgent);
      if (requestedAgent !== agent) {
        return;
      }

      files = listed;
      const first = listed.find(file => file.exists);
      if (first) {
        await open(first);
      }
    })();
  });

  // Config has no count or refresh — clear those slots in the shared header.
  $effect(() => {
    setPanelHeader({
      count: null,
      refresh: null
    });
  });
</script>

<div class="cfg">
  <div class="scroll">
    {#each files as f (f.rel)}
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
    {/each}

    <section class="viewer">
      <div class="card">
        <h3 class:placeholder={!selected}>{selected?.rel ?? "Select a file to view"}</h3>
        <pre class="body"><ColorText text={content} vars={fileVars} /></pre>
      </div>
      <p class="note">Read-only in the MVP — edits will write back to this same file.</p>
    </section>
  </div>
</div>

<style>
  .cfg {
    display: flex;
    flex-direction: column;
    block-size: 100%;
  }

  .scroll {
    display: flex;
    flex: 1;
    flex-direction: column;
    gap: 10px;
    overflow-y: auto;
    min-block-size: 0;
    padding: 10px;
    animation: panel-swap 280ms var(--ease);
  }

  .row {
    display: flex;
    gap: 10px;
    align-items: center;
    inline-size: 100%;
    padding-block: 8px;
    padding-inline: 10px;
    border: none;
    border-radius: var(--radius-small);
    background: transparent;
    color: var(--on-surface);
    text-align: start;
    cursor: pointer;
    transition: background 140ms var(--ease);

    &:hover:not(:disabled) {
      background: var(--surface-2);
    }

    &:disabled {
      opacity: 45%;
      cursor: default;
    }

    &.sel {
      background: var(--primary-container);
      color: var(--on-primary-container);
    }
  }

  .kind {
    flex: none;
    padding-block: 2px;
    padding-inline: 9px;
    border-radius: 999px;
    background: var(--surface-3);
    color: var(--on-surface-variant);
    font-weight: 700;
    font-size: 10px;
    letter-spacing: 0.05em;
    text-transform: uppercase;

    &.instructions {
      background: var(--tertiary-wash);
      color: var(--tertiary);
    }

    &.mcp {
      background: var(--primary-container);
      color: var(--on-primary-container);
    }
  }

  .rel {
    overflow: hidden;
    font-family: var(--font-monospace);
    font-size: 12px;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .missing {
    margin-inline-start: auto;
    color: var(--on-surface-variant);
    font-style: italic;
    font-size: 10px;
  }

  .viewer {
    display: flex;
    flex-direction: column;
    gap: 2px;
    margin-block-start: 6px;

    .card {
      overflow: hidden;
      border: 1px solid var(--outline);
      border-radius: var(--radius-medium);
    }

    h3 {
      margin: 0;
      padding-block: 8px;
      padding-inline: 12px;
      background: var(--surface-2);
      color: var(--on-surface);
      font-family: var(--font-monospace);
      font-weight: 600;
      font-size: 12px;
    }

    .placeholder {
      color: var(--on-surface-variant);
      font-style: italic;
    }

    .body {
      overflow: auto;
      max-block-size: 280px;
      margin: 0;
      padding: 12px;
      background: var(--code-background);
      color: var(--code-foreground);
      font-family: var(--font-monospace);
      font-size: 12px;
      line-height: 1.55;
      white-space: pre-wrap;
    }

    .note {
      margin-block: 2px 0;
      margin-inline: 0;
      color: var(--on-surface-variant);
      font-style: italic;
      font-size: 11px;
    }
  }
</style>
