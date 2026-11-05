<script lang="ts">
  // The small uppercase heading over each Config section - a settings card's
  // title and the project-files list's alike. Keep the `card-label` class: the
  // Cyberpunk skin (theme.css) draws its accent stub under it. `section-label`
  // is the shared label writing; ProjectCard's title shares only the stub.
  const { text }: { text: string } = $props();
</script>

<h3 class="card-label section-label">{text}</h3>

<style>
  .card-label {
    margin: 0;
    font-size: 0.625rem;
  }
</style>
