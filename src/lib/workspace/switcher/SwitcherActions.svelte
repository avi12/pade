<script lang="ts">
  import Icon from "@/lib/Icon.svelte";
  import { WindowMode } from "@/lib/types";

  // The left rail's window-level actions: open the full picker, and spawn a new
  // empty or throwaway window or the agents dashboard. Per-project actions
  // (rename, browse, pin, delete) live on each project row's own menu. A window
  // with no project open (the picker itself) offers only the spawns.
  const { onswitch, onspawn }: {
    /** Open the full picker (browse every root, clone, open a folder) - absent
     *  where the picker is already on screen. */
    onswitch?: () => void;
    /** Spawn a fresh window in this mode. */
    onspawn: (mode: WindowMode) => void;
  } = $props();
</script>

{#if onswitch !== undefined}
  <button class="menu-item action" onclick={onswitch} role="menuitem" type="button">
    <span class="lead"><Icon name="swap" /></span>
    <span class="grow">Open a project…</span>
    <span class="subtitle">All projects &amp; clone</span>
  </button>
  <div class="menu-divider"></div>
{/if}

<div class="menu-separator">New window</div>
<button
  class="menu-item action"
  onclick={() => onspawn(WindowMode.enum.empty)}
  role="menuitem"
  type="button"
>
  <span class="lead accent"><Icon name="windowPlus" /></span>
  <span class="grow">Empty window</span>
  <span class="kbd">Ctrl ⇧ N</span>
</button>
<button
  class="menu-item action"
  onclick={() => onspawn(WindowMode.enum.temp)}
  role="menuitem"
  type="button"
>
  <span class="lead tertiary"><Icon name="plus" /></span>
  <span class="grow">Throwaway workspace</span>
</button>
<button
  class="menu-item action"
  onclick={() => onspawn(WindowMode.enum.dashboard)}
  role="menuitem"
  type="button"
>
  <span class="lead"><Icon name="activity" /></span>
  <span class="grow">Agents dashboard</span>
  <span class="subtitle">Every window at a glance</span>
</button>

<style>
  /* The row itself is the shared .menu-item; these rows only weigh heavier and
     light up on keyboard focus too. */
  .action {
    gap: 10px;
    font-weight: 600;

    &:focus-visible {
      background: var(--primary-container);
      color: var(--on-primary-container);
      outline: none;
    }

    .grow {
      flex: 1;
      min-inline-size: 0;
    }

    .lead {
      display: inline-flex;
      flex: none;
      color: var(--on-surface-variant);

      &.accent {
        color: var(--primary);
      }

      &.tertiary {
        color: var(--tertiary);
      }
    }

    .subtitle {
      flex: none;
      color: var(--on-surface-variant);
      font-weight: 600;
      font-size: 10px;
    }

    .kbd {
      flex: none;
      padding-block: 2px;
      padding-inline: 6px;
      border-radius: 6px;
      background: var(--surface-3);
      color: var(--on-surface-variant);
      font-family: var(--font-monospace);
      font-size: 10px;
    }

    &:hover .lead,
    &:focus-visible .lead,
    &:hover .subtitle,
    &:focus-visible .subtitle,
    &:hover .kbd,
    &:focus-visible .kbd {
      color: inherit;
    }
  }
</style>
