<script lang="ts">
  // A model's effort ladder as one row of buttons, each showing its level as the
  // chip in the colour a tab tints its model name - so the row runs cool to hot.

  import EffortTint from "@/lib/model/EffortTint.svelte";
  import { effortTone } from "@/lib/model/model-label";
  import type { ModelOption } from "@/lib/types";

  const { efforts, current, recommended, onpick }: {
    efforts: ModelOption["efforts"];
    /** The level in use (or picked), highlighted. */
    current: string | null;
    /** The level the agent recommends for this model, marked. */
    recommended: string | null;
    onpick: (effort: string) => Promise<void>;
  } = $props();
</script>

<div class="efforts" aria-label="Reasoning effort" role="group">
  {#each efforts as effort, i (effort.id)}
    {@const isCurrent = current === effort.id}
    {@const isRecommended = recommended === effort.id}
    {@const tone = effortTone({
      step: i + 1,
      steps: efforts.length
    })}
    <button
      class="effort"
      class:current={isCurrent}
      class:recommended={isRecommended}
      aria-pressed={isCurrent}
      data-tooltip={isRecommended ? `${effort.name} - recommended` : effort.name}
      onclick={async () => await onpick(effort.id)}
    >
      <EffortTint isFilled {tone}>{effort.name}</EffortTint>
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
    display: grid;
    justify-items: center;
    padding: 6px 4px;
    border: 1px solid transparent;
    border-radius: var(--radius-small);
    background: transparent;
    font-family: var(--font-monospace);
    font-size: 0.625rem;
    cursor: pointer;

    &:hover {
      background: var(--surface-3);
    }

    &.current {
      border-color: var(--primary);
      background: var(--surface-3);
    }

    &.recommended {
      text-decoration: underline dotted;
      text-underline-offset: 3px;
    }
  }
</style>
