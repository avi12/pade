<script lang="ts">
  // A model's effort ladder as the design's segmented control: one button per
  // level, each a single bar climbing in height over its name - the tab's
  // signal bars spread out, one per level, lit in the primary colour up to the
  // level in use. The level in use sits on the primary container.

  import { effortBars } from "@/lib/model/model-label";
  import type { ModelOption } from "@/lib/types";

  const { efforts, current, recommended, onpick }: {
    efforts: ModelOption["efforts"];
    /** The level in use (or picked), highlighted. */
    current: string | null;
    /** The level the agent recommends for this model, marked. */
    recommended: string | null;
    onpick: (effort: string) => Promise<void>;
  } = $props();

  const ladder = $derived(efforts.map(effort => effort.id));
  // Each level's bar height, read off the ladder's top level so every bar
  // climbs as a tab's do; lit up to the level in use, none while none is.
  const bars = $derived.by(() => {
    const iCurrent = current === null ? -1 : ladder.indexOf(current);
    const heights = effortBars({
      model: "",
      effort: ladder.at(-1) ?? "",
      ladder
    });
    return heights.map((bar, i) => ({
      heightPercent: bar.heightPercent,
      isLit: i <= iCurrent
    }));
  });
</script>

<div class="efforts" aria-label="Reasoning effort" role="group">
  {#each efforts as effort, i (effort.id)}
    {@const isCurrent = current === effort.id}
    {@const isRecommended = recommended === effort.id}
    <button
      class="effort"
      class:current={isCurrent}
      class:recommended={isRecommended}
      aria-pressed={isCurrent}
      data-tooltip={isRecommended ? `${effort.name} - recommended` : effort.name}
      onclick={async () => await onpick(effort.id)}
    >
      <span class="bar-slot" aria-hidden="true">
        <span style:--bar-height="{bars[i]?.heightPercent ?? 100}%" class="bar" class:lit={bars[i]?.isLit}></span>
      </span>
      <span class="name">{effort.name}</span>
    </button>
  {/each}
</div>

<style>
  .efforts {
    display: flex;
    gap: 3px;
    margin-block-end: 6px;
    margin-inline: 6px;
    padding: 3px;
    border: 1px solid var(--outline);
    border-radius: var(--radius-medium);
    background: var(--surface-1);
  }

  .effort {
    display: flex;
    flex: 1;
    flex-direction: column;
    gap: 4px;
    align-items: center;
    min-inline-size: 0;
    padding-block: 6px 5px;
    padding-inline: 2px;
    border: none;
    border-radius: var(--radius-small);
    background: transparent;
    color: var(--on-surface-variant);
    cursor: pointer;
    transition: background 120ms var(--ease), color 120ms var(--ease);

    &:hover {
      background: var(--surface-3);
      color: var(--on-surface);
    }

    &.current {
      background: var(--primary-container);
      color: var(--on-primary-container);
    }
  }

  .bar-slot {
    display: flex;
    align-items: flex-end;
    block-size: 0.875rem;
  }

  .bar {
    block-size: var(--bar-height);
    inline-size: 0.5rem;
    border-radius: var(--radius-small);
    background: var(--outline);

    &.lit {
      background: var(--primary);
    }
  }

  .name {
    font-family: var(--font-monospace);
    font-weight: 700;
    font-size: 0.65625rem;

    .recommended > & {
      text-decoration: underline dotted;
      text-underline-offset: 3px;
    }
  }
</style>
