<script lang="ts">
  // The small pill that marks a throwaway workspace wherever one is named: the
  // switcher's trigger, its save card, window rows and project rows.
  const BADGE_TEXT = "temp";
</script>

<span class="temp-badge">{BADGE_TEXT}</span>

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
