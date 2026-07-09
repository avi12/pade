<script lang="ts">
  import { onMount } from "svelte";
  import { ide } from "./bridge";
  import type { Ide } from "./types";

  // Opens the active project in an external editor. The list is ranked for the
  // project type (suggest), so the best-fit IDE sits at the top.
  let ides = $state<Ide[]>([]);

  onMount(async () => {
    ides = await ide.suggest();
  });
</script>

{#if ides.length}
  <details class="ide-menu">
    <summary>Open in {ides[0].label}<span class="caret">▾</span></summary>
    <ul>
      {#each ides as i, idx (i.id)}
        <li>
          <button onclick={() => ide.open(i.command)}>
            {i.label}
            {#if idx === 0}<span class="best">best fit</span>{/if}
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
      list-style: none;
      display: inline-flex;
      align-items: center;
      gap: 6px;
      font-size: 13px;
      font-weight: 600;
      color: var(--on-surface);
      background: var(--surface-2);
      padding: 7px 14px;
      border-radius: 999px;
      cursor: pointer;
      user-select: none;
    }
    summary::-webkit-details-marker { display: none; }
    summary:hover { background: var(--surface-3); }
    .caret { color: var(--on-surface-var); font-size: 10px; }

    ul {
      position: absolute;
      inset-block-start: calc(100% + 6px);
      inset-inline-end: 0;
      z-index: 10;
      min-inline-size: 200px;
      margin: 0;
      padding: 6px;
      list-style: none;
      background: var(--surface-2);
      border: 1px solid var(--outline);
      border-radius: var(--r-md);
      box-shadow: 0 8px 24px color-mix(in srgb, var(--on-surface) 20%, transparent);
    }
    li button {
      inline-size: 100%;
      display: flex;
      align-items: center;
      gap: 8px;
      text-align: start;
      font: inherit;
      font-size: 13px;
      color: var(--on-surface);
      background: transparent;
      border: none;
      padding: 8px 10px;
      border-radius: var(--r-sm);
      cursor: pointer;

      &:hover { background: var(--primary-container); color: var(--on-primary-container); }
      .best {
        margin-inline-start: auto;
        font-size: 10px;
        font-weight: 700;
        text-transform: uppercase;
        letter-spacing: 0.06em;
        color: var(--tertiary);
      }
    }
  }
</style>
