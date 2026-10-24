<script lang="ts">
  // The small uppercase heading over each Config section - a settings card's
  // title and the project-files list's alike. Keep the `card-label` class: the
  // Cyberpunk skin (theme.css) draws its accent stub under it.
  const { text }: { text: string } = $props();
</script>

<h3 class="card-label">{text}</h3>

<style>
  .card-label {
    margin: 0;
    color: var(--on-surface-variant);
    font-weight: 700;
    font-size: 0.625rem;
    letter-spacing: 0.08em;
    text-transform: uppercase;
  }
</style>
