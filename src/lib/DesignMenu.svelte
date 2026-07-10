<script lang="ts">
  import { design } from "@/lib/bridge";
  import Icon from "@/lib/Icon.svelte";
  import type { DesignTool } from "@/lib/types";

  // Quick-launch an AI design/UI-generation tool (Claude, Google Stitch, v0, …)
  // as a design-to-code companion to the agent terminal. The roster is ranked for
  // the active agent — the vendor-matched tool is pinned first and flagged.
  // Picking a tool opens its live UI in a companion PADE window (a native webview
  // — the tools all block iframes), never bouncing to the external browser.
  const { agent }: { agent: string } = $props();

  let tools = $state<DesignTool[]>([]);

  // Re-rank whenever the active agent changes; ignore a stale in-flight response.
  $effect(() => {
    const active = agent;
    let cancelled = false;
    void (async () => {
      const list = await design.tools(active);
      if (!cancelled) {
        tools = list;
      }
    })();
    return () => {
      cancelled = true;
    };
  });
</script>

{#if tools.length}
  <button style:anchor-name="--design-anchor" class="design-btn" popovertarget="design-menu">
    <Icon name="sparkles" /> Design<span class="caret">▾</span>
  </button>
  <ul id="design-menu" style:position-anchor="--design-anchor" class="design-list" popover>
    <li class="hint">Open a design-to-code tool</li>
    {#each tools as tool (tool.id)}
      <li>
        <button onclick={() => void design.open(tool.url)} popovertarget="design-menu" popovertargetaction="hide">
          <span class="tool">
            <Icon name="star" />{tool.label}
            {#if tool.recommended}
              <span class="best">best fit</span>
            {/if}
          </span>
          <span class="vendor">{tool.vendor}</span>
        </button>
      </li>
    {/each}
  </ul>
{/if}

<style>
  .design-btn {
    display: inline-flex;
    gap: 6px;
    align-items: center;
    padding: 7px 13px;
    border: none;
    border-radius: 999px;
    background: var(--surface-2);
    color: var(--on-surface);
    font: inherit;
    font-weight: 600;
    font-size: 13px;
    cursor: pointer;

    &:hover {
      background: var(--surface-3);
    }

    .caret {
      font-size: 10px;
      opacity: 70%;
    }
  }

  /* Native popover — light-dismisses on outside click. */
  .design-list {
    position: absolute;
    inset: auto;
    min-inline-size: 230px;
    margin-block: 6px 0;
    margin-inline: 0;
    padding: 6px;
    border: 1px solid var(--outline);
    border-radius: var(--radius-medium);
    background: var(--surface-2);
    list-style: none;
    box-shadow: 0 16px 40px var(--shadow-color);
    position-area: bottom span-left;

    .hint {
      padding-block: 6px 4px;
      padding-inline: 10px;
      color: var(--on-surface-variant);
      font-weight: 700;
      font-size: 10px;
      letter-spacing: 0.08em;
      text-transform: uppercase;
    }

    li button {
      display: flex;
      gap: 10px;
      justify-content: space-between;
      align-items: center;
      inline-size: 100%;
      padding: 8px 10px;
      border: none;
      border-radius: var(--radius-small);
      background: transparent;
      color: var(--on-surface);
      font: inherit;
      font-size: 13px;
      text-align: start;
      cursor: pointer;

      &:hover {
        background: var(--primary-container);
        color: var(--on-primary-container);
      }

      .tool {
        display: flex;
        gap: 8px;
        align-items: center;
        font-weight: 600;
      }

      .best {
        color: var(--tertiary);
        font-weight: 700;
        font-size: 9px;
        letter-spacing: 0.06em;
        text-transform: uppercase;
      }

      .vendor {
        color: var(--on-surface-variant);
        font-size: 11px;
      }

      &:hover .best {
        color: inherit;
      }

      &:hover .vendor {
        color: inherit;
        opacity: 75%;
      }
    }
  }
</style>
