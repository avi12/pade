<script lang="ts">
  // The small uppercase heading over each Config section - a settings card's
  // title and the project-files list's alike.
  const { text }: { text: string } = $props();
</script>

<h3 class="section-label">{text}</h3>

<style>
  .section-label {
    margin: 0;
    color: var(--on-surface-variant);
    font-weight: 700;
    font-size: 10px;
    letter-spacing: 0.08em;
    text-transform: uppercase;
  }
</style>
