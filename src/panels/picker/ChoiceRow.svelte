<script generics="Value" lang="ts">
  import { rovingTablist } from "@/lib/ui/roving-tabs";
  import type { LabelledChoice } from "@/panels/picker/on-launch";

  // One home for the On-launch single-choice row: a caption beside a pill
  // tablist, the picked pill the one Tab stop (arrows move between them).
  const { caption, choices, selected, onpick }: {
    caption: string;
    choices: readonly LabelledChoice<Value>[];
    selected: Value;
    onpick: (value: Value) => void;
  } = $props();
</script>

<div class="choice-row">
  <span class="caption">{caption}</span>
  <div class="pill-tabs" role="tablist" use:rovingTablist>
    {#each choices as choice (choice.value)}
      {@const isSelected = choice.value === selected}
      <button
        class="pill-tab"
        aria-selected={isSelected}
        onclick={() => onpick(choice.value)}
        role="tab"
        tabindex={isSelected ? 0 : -1}
      >{choice.label}</button>
    {/each}
  </div>
</div>

<style>
  /* The pill-tabs chrome is shared (chrome.css). */
  .choice-row {
    display: flex;
    flex-wrap: wrap;
    gap: 12px;
    align-items: center;
  }

  .caption {
    color: var(--on-surface-variant);
    font-size: 0.8125rem;
  }
</style>
