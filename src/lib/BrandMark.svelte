<script lang="ts">
  // The PADE wordmark lockup — the logo badge beside "PADE": one shared home so
  // every surface (onboarding card, project-picker header, …) renders the brand
  // identically (DRY); the 6px gap supplies the spacing, so there is no literal space.
  import Logo from "@/lib/Logo.svelte";
</script>

<span class="brand"><Logo size={15} />PADE</span>

<style>
  .brand {
    display: inline-flex;
    gap: 6px;
    align-items: center;
    color: var(--primary);
    font-weight: 700;
    font-size: 13px;
    letter-spacing: 0.02em;
  }
</style>
