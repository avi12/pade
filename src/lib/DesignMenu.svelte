<script lang="ts">
  import { design } from "./bridge";
  import Icon from "./Icon.svelte";
  import type { DesignTool } from "./types";

  // Quick-launch an AI design/UI-generation tool (Claude, Google Stitch, v0, …)
  // as a design-to-code companion to the agent terminal. The roster is ranked for
  // the active agent — the vendor-matched tool is pinned first and flagged.
  const { agent }: { agent: string } = $props();

  let tools = $state<DesignTool[]>([]);

  // Re-rank whenever the active agent changes; ignore a stale in-flight response.
  $effect(() => {
    const active = agent;
    let cancelled = false;
    design.tools(active).then(list => {
      if (!cancelled) {
        tools = list;
      }
    });
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
    {#each tools as t (t.id)}
      <li>
        <button onclick={() => void design.open(t.url)} popovertarget="design-menu" popovertargetaction="hide">
          {t.label}
          {#if t.recommended}
            <span class="best">best fit</span>
          {/if}
          <span class="vendor">{t.vendor}</span>
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
    padding: 7px 14px;
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
      color: var(--on-surface-var);
      font-size: 10px;
    }
  }

  /* Native popover — light-dismisses on outside click. */
  .design-list {
    position: absolute;
    inset: auto;
    position-area: bottom span-left;
    min-inline-size: 220px;
    margin-block: 6px 0;
    margin-inline: 0;
    padding: 6px;
    border: 1px solid var(--outline);
    border-radius: var(--r-md);
    background: var(--surface-2);
    list-style: none;
    box-shadow: 0 8px 24px color-mix(in sRGB, var(--on-surface) 20%, transparent);

    .hint {
      padding-block: 6px 4px;
      padding-inline: 10px;
      color: var(--on-surface-var);
      font-size: 11px;
      letter-spacing: 0.04em;
    }

    li button {
      display: flex;
      gap: 8px;
      align-items: baseline;
      inline-size: 100%;
      padding: 8px 10px;
      border: none;
      border-radius: var(--r-sm);
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

      .best {
        color: var(--tertiary);
        font-weight: 700;
        font-size: 10px;
        letter-spacing: 0.06em;
        text-transform: uppercase;
      }

      .vendor {
        margin-inline-start: auto;
        color: var(--on-surface-var);
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
