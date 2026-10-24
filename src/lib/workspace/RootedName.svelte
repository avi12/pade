<script lang="ts">
  // A name with its root before it ("avi\pade" on Windows, "code/pade"
  // elsewhere) - the top bar's label, the root quieter so the name still
  // leads. One home for the format the left rail's rows and the composer's
  // project menu show; `projectRootPrefix` / `directoryPrefix` give the root
  // with the path's own separator. An empty prefix shows the name alone.
  const { prefix, name }: {
    prefix: string;
    name: string;
  } = $props();
</script>

{#if prefix}
  <span class="root">{prefix}</span>
{/if}{name}

<style>
  .root {
    color: var(--on-surface-variant);
    font-weight: 400;
  }
</style>
