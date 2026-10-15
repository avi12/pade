<script lang="ts">
  import Icon from "@/lib/Icon.svelte";
  import { WindowMode } from "@/lib/types";
  import { shortDisplayName } from "@/lib/util/paths";

  // The actions under the switcher's project list: open the full picker, rename
  // this window's folder, and spawn a new empty or throwaway window.
  const { path, labels, onswitch, onrenamefolder, onspawn }: {
    /** This window's project. */
    path: string;
    labels: Record<string, string>;
    /** Open the full picker (browse every root, clone, open a folder). */
    onswitch: () => void;
    /** Rename this window's folder on disk. */
    onrenamefolder: () => void;
    /** Spawn a fresh window in this mode. */
    onspawn: (mode: WindowMode) => void;
  } = $props();
</script>

<button class="action" onclick={onswitch} role="menuitem" type="button">
  <span class="lead"><Icon name="swap" /></span>
  <span class="grow">Open a project…</span>
  <span class="subtitle">All projects &amp; clone</span>
</button>
<button class="action" onclick={onrenamefolder} role="menuitem" type="button">
  <span class="lead"><Icon name="folder" /></span>
  <span class="grow">Rename this folder…</span>
  <span class="subtitle">{shortDisplayName({
    path,
    labels
  })}</span>
</button>

<div class="menu-divider"></div>

<div class="menu-separator">New window</div>
<button
  class="action"
  onclick={() => onspawn(WindowMode.enum.empty)}
  role="menuitem"
  type="button"
>
  <span class="lead accent"><Icon name="windowPlus" /></span>
  <span class="grow">Empty window</span>
  <span class="kbd">Ctrl ⇧ N</span>
</button>
<button
  class="action"
  onclick={() => onspawn(WindowMode.enum.temp)}
  role="menuitem"
  type="button"
>
  <span class="lead tertiary"><Icon name="plus" /></span>
  <span class="grow">Throwaway workspace</span>
</button>

<style>
  .action {
    display: flex;
    gap: 10px;
    align-items: center;
    inline-size: 100%;
    padding-block: 8px;
    padding-inline: 10px;
    border: none;
    border-radius: var(--radius-small);
    background: transparent;
    color: var(--on-surface);
    font: inherit;
    font-weight: 600;
    font-size: 13px;
    text-align: start;
    cursor: pointer;
    transition: color 120ms var(--ease), background 120ms var(--ease);

    &:hover,
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
