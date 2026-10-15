<script lang="ts">
  import { design } from "@/lib/bridge";
  import Icon from "@/lib/Icon.svelte";
  import type { DesignTool } from "@/lib/types";

  // Quick-launch an AI design/UI-generation tool (Claude, Google Stitch, v0, …)
  // as a design-to-code companion to the agent terminal. The roster is ranked for
  // the active agent - the vendor-matched tool is pinned first and flagged.
  // Picking a tool opens its live UI in a companion PADE window (a native webview
  // - the tools all block iframes), never bouncing to the external browser.
  const { agent }: { agent: string } = $props();

  const MENU_ID = "design-menu";
  const MENU_ANCHOR = "--design-anchor";

  let tools = $state<DesignTool[]>([]);

  // Re-rank whenever the active agent changes; ignore a stale in-flight response.
  $effect(() => {
    const active = agent;
    let isCancelled = false;
    async function loadTools() {
      const ranked = await design.tools(active);
      if (!isCancelled) {
        tools = ranked;
      }
    }
    loadTools();
    return () => {
      isCancelled = true;
    };
  });
</script>

{#if tools.length > 0}
  <span class="menu-host">
    <button style:anchor-name={MENU_ANCHOR} class="design-button menu-trigger" popovertarget={MENU_ID}>
      <Icon name="sparkles" /> Design<span class="caret">▾</span>
    </button>
    <ul id={MENU_ID} style:position-anchor={MENU_ANCHOR} class="design-list popover-menu" popover>
      <li class="menu-separator">Open a design-to-code tool</li>
      {#each tools as tool (tool.id)}
        <li>
          <button
            class="menu-item"
            onclick={async () => await design.open(tool.id)}
            popovertarget={MENU_ID}
            popovertargetaction="hide"
          >
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
  </span>
{/if}

<style>
  /* Layout-neutral wrapper so the shared .menu-host rule can hold the trigger
     active while #design-menu is open. */
  .menu-host {
    display: contents;
  }

  .design-button {
    display: inline-flex;
    gap: 6px;
    align-items: center;
    padding: 7px 13px;
    border: none;
    border-radius: var(--radius-full);
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

  /* Shell and rows come from the shared .popover-menu and .menu-item; only
     width, anchor side and the tool / vendor split live here. */
  .design-list {
    min-inline-size: 230px;
    position-area: bottom span-left;

    .menu-item {
      gap: 10px;
      justify-content: space-between;

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
