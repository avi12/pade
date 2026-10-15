<script lang="ts">
  import type { HTMLAttributes } from "svelte/elements";

  // The small pill that marks a throwaway workspace wherever one is named: the
  // switcher's trigger, its save card, window rows, project rows and the
  // picker's recent list. Extra attributes and attachments (a tooltip) pass
  // through to the pill.
  const BADGE_TEXT = "temp";

  const { ...attributes }: HTMLAttributes<HTMLSpanElement> = $props();
</script>

<span {...attributes} class="temp-badge">{BADGE_TEXT}</span>

<style>
  .temp-badge {
    flex: none;
    padding-block: 1px;
    padding-inline: 6px;
    border-radius: var(--radius-full);
    background: var(--surface-3);
    color: var(--on-surface-variant);
    font-family: var(--font-ui);
    font-weight: 700;
    font-size: 9px;
    letter-spacing: 0.06em;
    text-transform: uppercase;
  }
</style>
