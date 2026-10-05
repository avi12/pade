<script lang="ts">
  import { formatAge, formatTimestamp } from "@/lib/format";
  import { settings } from "@/lib/settings.svelte";
  import { tooltip } from "@/lib/truncation-tooltip";

  // When a project was last opened or left, as a compact age ("3d ago") with the
  // exact time on hover. Shared by the launcher's Recent list and the switcher so
  // both read the one backend stamp. `now` is the moment the list was shown —
  // the age is a glance, not a ticking clock. Renders nothing for a project PADE
  // has no record of yet.
  const { path, now }: {
    path: string;
    now: number;
  } = $props();

  const lastUsed = $derived(settings.workspaceLastUsed[path]);
</script>

{#if lastUsed !== undefined}
  <time
    class="last-accessed"
    {@attach tooltip(`Last accessed ${formatTimestamp(lastUsed)}`)}
    datetime={new Date(lastUsed).toISOString()}
  >{formatAge({
    stamp: lastUsed,
    now
  })} ago</time>
{/if}

<style>
  /* A host list sizes and recolors it through these two properties (the
     switcher's denser rows, and its hover inversion). */
  .last-accessed {
    flex: none;
    margin-inline-start: auto;
    color: var(--last-accessed-color, var(--on-surface-variant));
    font-family: var(--font-monospace);
    font-size: var(--last-accessed-font-size, 11px);
    white-space: nowrap;
  }
</style>
