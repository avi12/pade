<script lang="ts">
  import Icon from "@/lib/Icon.svelte";
  import { collapseRow } from "@/lib/motion";
  import { displayName, isTemporaryWorkspace } from "@/lib/paths";
  import type { Ide } from "@/lib/types";
  import type { WorkspaceLifecycle } from "@/panels/picker/lifecycle.svelte";
  import RowMenu from "@/panels/picker/RowMenu.svelte";

  // Recent projects: open-on-click rows with temp/project tags, the shared
  // row menu, and the inline-rename form driven by the shared lifecycle.
  // Renders nothing until there is history.
  const {
    recentProjects,
    labels,
    ides,
    lifecycle,
    onopen,
    onclear
  }: {
    recentProjects: string[];
    /** Friendly display labels per path (temp workspaces get auto-named). */
    labels: Record<string, string>;
    ides: Ide[];
    lifecycle: WorkspaceLifecycle;
    onopen: (target: { path: string }) => void;
    onclear: () => void;
  } = $props();
</script>

{#if recentProjects.length > 0}
  <section class="recent">
    <div class="recent-head">
      <h2>Recent</h2>
      <button class="clear" onclick={onclear}><Icon name="trash" /> Clear</button>
    </div>
    <ul class="recent-list">
      <!-- A deleted (or cleared) row collapses on its way out instead of
           blinking away, so the list visibly closes over it. -->
      {#each recentProjects as path (path)}
        <li class="row" out:collapseRow>
          {#if lifecycle.renaming === path}
            <form
              class="rename" onsubmit={async e => {
                e.preventDefault(); await lifecycle.commitRename(path);
              }}>
              <input
                aria-describedby={lifecycle.renameError ? "rename-error" : undefined}
                aria-invalid={lifecycle.renameError !== null}
                aria-label="Project name"
                bind:value={lifecycle.renameValue}
              />
              <button disabled={!lifecycle.renameValid} type="submit">Save</button>
              <button onclick={() => lifecycle.cancelRename()} type="button">Cancel</button>
              {#if lifecycle.renameError}
                <output id="rename-error" class="field-error rename-error">{lifecycle.renameError}</output>
              {/if}
            </form>
          {:else}
            <button class="recent-item" onclick={() => onopen({ path })}>
              {#if isTemporaryWorkspace(path)}
                <span
                  class="temp-tag"
                  data-tooltip="Auto-named by the agent — the folder keeps its path"
                >temp</span>
              {:else if lifecycle.isOwned(path)}
                <span class="project-tag">project</span>
              {/if}
              <span class="rname">{displayName(path, labels)}</span>
              <span class="rpath">{path}</span>
            </button>
            <RowMenu {ides} {lifecycle} {path} scope="recent" />
          {/if}
        </li>
      {/each}
    </ul>
  </section>
{/if}

<style>
  .recent-head {
    display: flex;
    gap: 8px;
    justify-content: space-between;
    align-items: center;
  }

  .clear {
    display: inline-flex;
    gap: 5px;
    align-items: center;
    padding: 4px 8px;
    border: none;
    border-radius: 999px;
    background: transparent;
    color: var(--on-surface-variant);
    font: inherit;
    font-size: 12px;
    cursor: pointer;
    transition:
      color 150ms var(--ease),
      background 150ms var(--ease);

    &:hover {
      background: var(--critical-wash);
      color: var(--critical);
    }
  }

  /* Tighter than the shared 12px picker-section gap — canon groups the head and
     rows at 8px, and the rows sit 8px apart. */
  .recent {
    gap: 8px;
  }

  .recent-list {
    display: flex;
    flex-direction: column;
    gap: 8px;
    margin: 0;
    padding: 0;
    list-style: none;
  }

  /* Recent row — pill button, mono name, truncating path; fills on hover. */
  .recent-item {
    display: flex;
    gap: 10px;
    align-items: center;
    inline-size: 100%;
    padding: 10px 12px;
    border: none;
    border-radius: var(--radius-medium);
    background: transparent;
    color: var(--on-surface);
    text-align: start;
    cursor: pointer;
    transition: background 150ms var(--ease);

    &:hover {
      background: var(--surface-2);
    }

    .rname {
      flex: none;
      font-family: var(--font-monospace);
      font-weight: 600;
      font-size: 13px;
    }

    .rpath {
      overflow: hidden;
      color: var(--on-surface-variant);
      font-family: var(--font-monospace);
      font-size: 11px;
      text-overflow: ellipsis;
      white-space: nowrap;
    }
  }

  .temp-tag {
    flex: none;
    padding: 2px 7px;
    border-radius: 999px;
    background: var(--surface-3);
    color: var(--on-surface-variant);
    font-weight: 700;
    font-size: 9px;
    letter-spacing: 0.06em;
    text-transform: uppercase;
  }

  .project-tag {
    flex: none;
    color: var(--tertiary);
    font-weight: 700;
    font-size: 9px;
    letter-spacing: 0.06em;
    text-transform: uppercase;
  }

  /* Inline rename — a bordered field with Save (primary) + Cancel. */
  .rename {
    display: flex;
    flex-wrap: wrap;
    gap: 8px;
    align-items: center;
    inline-size: 100%;
    padding: 6px 8px;
    border: 1px solid var(--primary);
    border-radius: var(--radius-medium);
    background: var(--surface-2);

    input {
      flex: 1;
      min-inline-size: 0;
      padding: 0;
      border: none;
      background: transparent;
      color: var(--on-surface);
      font-family: var(--font-monospace);
      font-weight: 600;
      font-size: 13px;
    }

    button {
      flex: none;
      padding: 6px 14px;
      border: none;
      border-radius: 999px;
      background: var(--primary);
      color: var(--on-primary);
      font: inherit;
      font-weight: 700;
      font-size: 12px;
      cursor: pointer;
      transition: filter 150ms var(--ease);

      &:hover:not(:disabled) {
        filter: brightness(1.06);
      }

      &:disabled {
        opacity: 50%;
        filter: none;
        cursor: default;
      }
    }

    button + button {
      padding: 6px 12px;
      background: transparent;
      color: var(--on-surface-variant);
      font-weight: 600;

      &:hover {
        background: var(--surface-3);
        filter: none;
      }
    }
  }

  /* The error sits on its own full-width line below the field + buttons
     (the row is flex; this breaks to a new line). */
  .rename-error {
    flex-basis: 100%;
    margin-block-start: 2px;
  }
</style>
