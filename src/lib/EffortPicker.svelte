<script lang="ts">
  // A model's effort ladder as one row of buttons, each showing the rising bars
  // filled to its own level — so the row climbs exactly like a tab's tag.

  import type { ModelOption } from "@/lib/bridge";
  import EffortBars from "@/lib/EffortBars.svelte";

  const { efforts, current, recommended, onpick }: {
    efforts: ModelOption["efforts"];
    /** The level in use (or picked), highlighted. */
    current: string | null;
    /** The level the agent recommends for this model, marked. */
    recommended: string | null;
    onpick: (effort: string) => Promise<void>;
  } = $props();

  function tooltip(effort: ModelOption["efforts"][number]): string {
    const isRecommended = effort.id === recommended;
    return isRecommended ? `${effort.name} - recommended` : effort.name;
  }
</script>

<div class="efforts" aria-label="Reasoning effort" role="group">
  {#each efforts as effort, index (effort.id)}
    <button
      class="effort"
      class:current={current === effort.id}
      class:recommended={recommended === effort.id}
      aria-pressed={current === effort.id}
      data-tooltip={tooltip(effort)}
      onclick={async () => await onpick(effort.id)}
    >
      <EffortBars step={index + 1} steps={efforts.length} />
      <span class="effort-name">{effort.name}</span>
    </button>
  {/each}
</div>

<style>
  .efforts {
    display: grid;
    grid-auto-columns: 1fr;
    grid-auto-flow: column;
    gap: 4px;
    padding-block: 2px 6px;
    padding-inline: 6px;
  }

  .effort {
    --effort-lit: var(--primary);

    display: grid;
    gap: 4px;
    justify-items: center;
    padding: 6px 4px;
    border: 1px solid transparent;
    border-radius: var(--radius-small);
    background: transparent;
    color: var(--on-surface-variant);
    font: inherit;
    cursor: pointer;

    &:hover {
      background: var(--surface-3);
    }

    &.current {
      --effort-lit: currentColor;

      border-color: var(--primary);
      background: var(--primary-container);
      color: var(--on-primary-container);
    }

    &.recommended .effort-name {
      text-decoration: underline dotted;
      text-underline-offset: 3px;
    }

    .effort-name {
      font-family: var(--font-monospace);
      font-size: 10px;
      white-space: nowrap;
    }
  }
</style>
