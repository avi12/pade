<script lang="ts">
  import type { ExtensionCount } from "@/lib/feed/filter/type-filter";
  import { typeFilterLabel } from "@/lib/feed/filter/type-filter";
  import Icon from "@/lib/Icon.svelte";
  import { updatePrefs } from "@/lib/settings/prefs.svelte";
  import { formatCount } from "@/lib/util/format";
  import { pullWorkspace } from "@/lib/vcs/pull-workspace";
  import TypeFilterMenu from "@/panels/feed/toolbar/TypeFilterMenu.svelte";
  import type { SvelteSet } from "svelte/reactivity";

  // The feed's toolbar: the opt-in file-type filter (a pill that reads the
  // current selection and drops a native popover of per-extension checkboxes),
  // the "Hide ignored" switch where the project has a .gitignore, and "Sync
  // all", which fast-forwards the workspace from its remote.
  const { project, extensionCounts, selectedExtensions, hasRemote, hasGitignore, isHiding, hiddenCount }: {
    project: string;
    extensionCounts: ExtensionCount[];
    selectedExtensions: SvelteSet<string>;
    /** There is a remote to pull from - else "Sync all" stays hidden. */
    hasRemote: boolean;
    /** The project has a .gitignore - else "Hide ignored" stays hidden. */
    hasGitignore: boolean;
    /** The feed hides what the .gitignore ignores (on by default). */
    isHiding: boolean;
    /** How many held changes that hides right now. */
    hiddenCount: number;
  } = $props();

  // "Sync all" in-flight guard - disables the button and spins its icon so a
  // slow fetch can't be double-fired.
  let isSyncing = $state(false);
</script>

<div class="toolbar">
  <div class="typefilter menu-host">
    <button
      style:anchor-name="--type-anchor"
      class="typechip menu-trigger"
      popovertarget="type-filter"
    >
      <span class="caret">▽</span>
      <span class="label">{typeFilterLabel({
        counts: extensionCounts,
        selected: selectedExtensions
      })}</span>
    </button>
    <TypeFilterMenu id="type-filter" anchor="--type-anchor" {extensionCounts} {selectedExtensions} />
  </div>

  {#if hasGitignore}
    <button
      class="ignored"
      aria-pressed={isHiding}
      data-tooltip={isHiding
        ? `Hiding changes the .gitignore ignores (${formatCount(hiddenCount)}) - click to show them`
        : "Showing changes the .gitignore ignores - click to hide them"}
      onclick={async () => await updatePrefs({ hideIgnoredChanges: !isHiding })}
    >
      <Icon name="git" size={14} />
      Hide ignored
      {#if isHiding && hiddenCount > 0}
        <span class="hidden-count">{formatCount(hiddenCount)}</span>
      {/if}
    </button>
  {/if}

  {#if hasRemote}
    <button
      class="sync"
      data-tooltip="Fast-forward this workspace from origin"
      disabled={isSyncing}
      onclick={async () => {
        isSyncing = true;
        await pullWorkspace(project);
        isSyncing = false;
      }}
    >
      <span class="icon" class:spin={isSyncing}><Icon name="refresh" size={14} /></span>
      Sync all
    </button>
  {/if}
</div>

<style>
  .toolbar {
    display: flex;
    gap: 8px;
    align-items: center;
    padding-block: 0;
    padding-inline: 14px;
  }

  /* The pressed state + caret flip come from the shared .menu-host/.menu-trigger
     chrome (theme.css). */
  .typefilter {
    display: inline-flex;
  }

  /* Every toolbar action is a quiet outlined pill: tools, not content. */
  .typechip,
  .ignored,
  .sync {
    display: inline-flex;
    gap: 6px;
    align-items: center;
    border: 1px solid var(--outline);
    border-radius: var(--radius-full);
    background: transparent;
    color: var(--on-surface-variant);
    font-family: inherit;
    font-weight: 700;
    font-size: 0.6875rem;
    cursor: pointer;
    transition: background 120ms var(--ease);
  }

  .typechip {
    padding-block: 4px;
    padding-inline: 10px 8px;

    &:hover {
      background: var(--surface-3);
    }

    .caret {
      font-size: 0.625rem;
      transition: rotate 200ms var(--ease);
    }
  }

  /* A switch: pressed (the default) it wears the primary container, so the
     filter in force reads at a glance. */
  .ignored {
    padding-block: 4px;
    padding-inline: 8px 10px;

    &:hover {
      background: var(--surface-3);
    }

    &[aria-pressed="true"] {
      border-color: var(--primary);
      background: var(--primary-container);
      color: var(--on-primary-container);
    }

    .hidden-count {
      font-weight: 600;
      opacity: 0.8;
    }
  }

  .sync {
    margin-inline-start: auto;
    padding-block: 4px;
    padding-inline: 8px 10px;

    &:disabled {
      color: var(--on-surface-variant);
      cursor: default;
    }

    &:hover:not(:disabled) {
      background: var(--surface-3);
    }

    .icon {
      display: inline-flex;
      color: var(--primary);
    }

    /* Reuses the global `spin` keyframe (theme.css); the global
       reduced-motion reset disables it for those who ask. */
    .spin {
      animation: spin 900ms linear infinite;
    }
  }
</style>
