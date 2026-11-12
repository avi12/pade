<script lang="ts">
  // One chat-storage limit: its name and what it does beside its options,
  // stacked once the panel is too narrow for both.
  import Segmented from "@/lib/ui/controls/Segmented.svelte";

  const { label, hint, options, chosen, onchoose }: {
    label: string;
    hint: string;
    options: readonly {
      value: number | null;
      label: string;
    }[];
    chosen: number | null;
    onchoose: (value: number | null) => void;
  } = $props();
</script>

<div class="limit-row">
  <span class="field-text">
    <span class="field-label">{label}</span>
    <span class="field-hint">{hint}</span>
  </span>
  <div class="limit-choice">
    <Segmented {chosen} {label} {onchoose} {options} />
  </div>
</div>

<style>
  .limit-row {
    /* Label beside the options while the panel is wide; stacked once the drag
       handle takes that room away. Asked of the card, never the viewport. */
    @container (inline-size < 420px) {
      flex-direction: column;
      align-items: stretch;
    }

    display: flex;
    gap: 12px;
    justify-content: space-between;
    align-items: center;
  }

  .limit-choice {
    flex: none;
    min-inline-size: 230px;
  }
</style>
