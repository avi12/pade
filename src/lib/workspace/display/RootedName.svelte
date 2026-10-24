<script lang="ts">
  // A name with its root before it ("avi/pade") - the top bar's label, the
  // root quieter so the name still leads. One home for the format the left
  // rail's rows and the composer's project menu show; `projectRootName`
  // derives a project's root. An empty root shows the name alone.
  const { root, name }: {
    root: string;
    name: string;
  } = $props();
</script>

{#if root}
  <span class="root">{root}/</span>
{/if}{name}

<style>
  .root {
    color: var(--on-surface-variant);
    font-weight: 400;
  }
</style>
