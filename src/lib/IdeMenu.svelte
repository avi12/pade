<script lang="ts">
  import { ide } from "./bridge";
  import Icon from "./Icon.svelte";
  import type { Ide } from "./types";
  import { onMount } from "svelte";

  // Opens the active project in an external editor. The list is ranked for the
  // project type (suggest), so the best-fit IDE sits at the top.
  let ides = $state<Ide[]>([]);

  onMount(async () => {
    ides = await ide.suggest();
  });
</script>

{#if ides.length}
  <details class="ide-menu">
    <summary><Icon name="external" /> Open in {ides[0].label}<span class="caret">▾</span></summary>
    <ul>
      {#each ides as i, idx (i.id)}
        <li>
          <button onclick={() => ide.open({ command: i.command })}>
            {i.label}
            {#if idx === 0}
              <span class="best">best fit</span>
            {/if}
          </button>
        </li>
      {/each}
    </ul>
  </details>
{/if}

<style>
  .ide-menu {
    position: relative;

    summary {
      display: inline-flex;
      gap: 6px;
      align-items: center;
      padding: 7px 14px;
      border-radius: 999px;
      background: var(--surface-2);
      color: var(--on-surface);
      list-style: none;
      font-weight: 600;
      font-size: 13px;
      cursor: pointer;
      user-select: none;
    }

    summary::-webkit-details-marker {
      display: none;
    }

    summary:hover {
      background: var(--surface-3);
    }

    .caret {
      color: var(--on-surface-var);
      font-size: 10px;
    }

    ul {
      position: absolute;
      inset-block-start: calc(100% + 6px);
      inset-inline-end: 0;
      z-index: 10;
      min-inline-size: 200px;
      margin: 0;
      padding: 6px;
      border: 1px solid var(--outline);
      border-radius: var(--r-md);
      background: var(--surface-2);
      list-style: none;
      box-shadow: 0 8px 24px color-mix(in sRGB, var(--on-surface) 20%, transparent);
    }

    li button {
      display: flex;
      gap: 8px;
      align-items: center;
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
        margin-inline-start: auto;
        color: var(--tertiary);
        font-weight: 700;
        font-size: 10px;
        letter-spacing: 0.06em;
        text-transform: uppercase;
      }
    }
  }
</style>
