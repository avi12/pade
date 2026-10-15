<script lang="ts">
  import Logo from "@/lib/ui/Logo.svelte";
</script>

<!-- Loading phase: a calm branded ground so a booting window is never a
     blank void (also the safety net if a boot step stalls). -->
<div class="booting">
  <span class="brand" aria-label="PADE is starting"><Logo size={64} /></span>
</div>

<style>
  /* Booting ground - a centered, gently pulsing brand mark on the app surface
     so a loading window reads as PADE, never a blank white void. */
  .booting {
    display: grid;
    place-items: center;
    block-size: 100%;
    background: radial-gradient(120% 120% at 50% 0%, var(--surface-1), var(--surface));

    .brand {
      display: inline-flex;
      animation: pulse 1400ms var(--ease) infinite;
    }
  }
</style>
