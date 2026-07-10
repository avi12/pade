<script lang="ts">
  import { windows } from "@/lib/bridge";
  import { formatCount } from "@/lib/format";
  import Icon from "@/lib/Icon.svelte";
  import Logo from "@/lib/Logo.svelte";
  import { displayName, isTemporaryWorkspace } from "@/lib/paths";
  import { FolderPath, parseInput } from "@/lib/validate";
  import { open as openDialog } from "@tauri-apps/plugin-dialog";

  // The project/app menu that leads the top bar: it names the current workspace
  // and hosts every "open something in a window" action — switch this window,
  // spawn an empty/throwaway window, jump into a recent project, or browse/open
  // a folder. All multi-window spawns funnel through `windows.create`, project
  // switching through the `onswitch` callback (which opens the picker).
  const {
    path,
    label,
    isTemp,
    recentProjects,
    labels,
    onswitch
  }: {
    path: string;
    label: string;
    isTemp: boolean;
    recentProjects: string[];
    labels: Record<string, string>;
    onswitch: () => void;
  } = $props();

  // The search box only earns its place once the list is long enough to warrant
  // filtering; a small list is faster to eyeball than to type through.
  const SEARCH_THRESHOLD = 6;
  const showSearch = $derived(recentProjects.length > SEARCH_THRESHOLD);

  let filter = $state("");

  // Clear the query when the search box is hidden, so a stale filter can't keep
  // the recents list narrowed once the box disappears.
  $effect(() => {
    if (!showSearch) {
      filter = "";
    }
  });

  const filtered = $derived.by(() => {
    const query = filter.trim().toLowerCase();
    if (!query) {
      return recentProjects;
    }

    return recentProjects.filter(
      projectPath =>
        displayName(projectPath, labels).toLowerCase().includes(query) || projectPath.toLowerCase().includes(query)
    );
  });
  const noMatch = $derived(filter.trim().length > 0 && filtered.length === 0);

  // Spawn a fresh window and dismiss the menu so it doesn't linger over the new
  // one. `path` is optional — omitted for empty/temp modes.
  async function spawn(args: {
    mode: "empty" | "temp" | "open";
    path?: string;
  }) {
    await windows.create(args);
    hide();
  }

  // Pick a folder, then open it in a new window. The picker returns null on
  // cancel; the path is validated at this trust boundary before it's forwarded.
  async function openFolder() {
    const picked = await openDialog({
      directory: true,
      multiple: false
    });
    const chosen = parseInput({
      schema: FolderPath,
      raw: picked
    });
    if (chosen) {
      await spawn({
        mode: "open",
        path: chosen
      });
    }
  }

  function switchHere() {
    hide();
    onswitch();
  }
  function browseAll() {
    hide();
    onswitch();
  }

  function hide() {
    const menu = document.getElementById("m-app");
    if (menu?.matches(":popover-open")) {
      menu.hidePopover();
    }
  }
</script>

<button
  class="trigger"
  aria-haspopup="menu"
  aria-label={`Project menu — ${label}`}
  data-tooltip={path}
  popovertarget="m-app"
>
  <Logo size={18} />
  <span class="stack">
    <span class="eyebrow">Project</span>
    <span class="name">
      {label}
      {#if isTemp}
        <span class="temp">temp</span>
      {/if}
    </span>
  </span>
  <span class="caret" aria-hidden="true">▾</span>
</button>

<div id="m-app" style:position-anchor="--m-app" class="menu" popover role="menu">
  <div class="head">
    <span class="chip"><Icon name="folder" /></span>
    <span class="ident">
      <span class="head-name">{label}</span>
      <span class="head-path">{path}</span>
    </span>
  </div>

  <button class="item" onclick={switchHere} role="menuitem">
    <span class="lead"><Icon name="swap" /></span>
    <span class="grow">Switch project in this window…</span>
  </button>

  <div class="sep"></div>

  <div class="eyebrow row">New window</div>
  <button class="item" onclick={() => spawn({ mode: "empty" })} role="menuitem">
    <span class="lead accent"><Icon name="windowPlus" /></span>
    <span class="grow">Empty window</span>
    <span class="kbd">Ctrl ⇧ N</span>
  </button>
  <button class="item" onclick={() => spawn({ mode: "temp" })} role="menuitem">
    <span class="lead tertiary"><Icon name="plus" /></span>
    <span class="grow">Throwaway workspace</span>
  </button>

  <div class="sep"></div>

  <div class="eyebrow row spread">
    <span>Open in a new window</span>
    <span class="count">{formatCount(recentProjects.length)}</span>
  </div>

  {#if showSearch}
    <label class="search">
      <span class="lead"><Icon name="search" /></span>
      <input aria-label="Filter recent projects" placeholder="Filter projects…" type="search" bind:value={filter} />
    </label>
  {/if}

  <div class="recents">
    {#each filtered as projectPath (projectPath)}
      <button
        class="item recent" onclick={() => spawn({
          mode: "open",
          path: projectPath
        })} role="menuitem">
        <span class="lead"><Icon name="folder" /></span>
        <span class="grow ident">
          <span class="head-name recent-name">
            {displayName(projectPath, labels)}
            {#if isTemporaryWorkspace(projectPath)}
              <span class="temp">temp</span>
            {/if}
          </span>
          <span class="head-path">{projectPath}</span>
        </span>
        <span class="trail"><Icon name="external" /></span>
      </button>
    {/each}
    {#if noMatch}
      <div class="empty">No projects match “{filter.trim()}”</div>
    {/if}
  </div>

  <button class="item browse" onclick={browseAll} role="menuitem">
    <span class="lead"><Icon name="history" /></span>
    <span class="grow">Browse all projects…</span>
  </button>

  <div class="sep"></div>

  <button class="item" onclick={openFolder} role="menuitem">
    <span class="lead"><Icon name="folder" /></span>
    <span class="grow">Open folder…</span>
  </button>
</div>

<style>
  .trigger {
    display: inline-flex;
    flex-shrink: 0;
    gap: 8px;
    align-items: center;
    padding-block: 5px;
    padding-inline: 11px 10px;
    border: none;
    border-radius: 999px;
    background: transparent;
    color: var(--on-surface);
    white-space: nowrap;
    cursor: pointer;
    transition: background 150ms var(--ease);
    anchor-name: --m-app;

    &:hover {
      background: var(--surface-2);
    }

    .stack {
      display: inline-flex;
      flex-direction: column;
      align-items: flex-start;
      line-height: 1.1;
    }

    .eyebrow {
      color: var(--on-surface-var);
      font-weight: 700;
      font-size: 9px;
      letter-spacing: 0.08em;
      text-transform: uppercase;
    }

    .name {
      display: inline-flex;
      gap: 6px;
      align-items: center;
      font-family: var(--font-mono);
      font-weight: 700;
      font-size: 14px;
    }

    .caret {
      color: var(--on-surface-var);
      font-weight: 600;
      font-size: 10px;
      opacity: 70%;
    }
  }

  /* A small temp pill, reused in the trigger and in each recent row. */
  .temp {
    padding-block: 1px;
    padding-inline: 6px;
    border-radius: 999px;
    background: var(--surface-3);
    color: var(--on-surface-var);
    font-family: var(--font-ui);
    font-weight: 700;
    font-size: 9px;
    letter-spacing: 0.06em;
    text-transform: uppercase;
  }

  /* Native popover — light-dismisses on outside click — anchored under the trigger. */
  .menu {
    position: absolute;
    inset: auto;
    min-inline-size: 272px;
    max-inline-size: 340px;
    margin-block: 6px 0;
    margin-inline: 0;
    padding: 6px;
    border: 1px solid var(--outline);
    border-radius: var(--r-md);
    background: var(--surface-2);
    color: var(--on-surface);
    box-shadow: 0 16px 40px var(--shadow-color);
    animation: pop-in 220ms var(--spring);
    position-area: bottom span-right;

    .head {
      display: flex;
      gap: 10px;
      align-items: center;
      padding-block: 8px 9px;
      padding-inline: 10px;

      .chip {
        display: inline-flex;
        flex-shrink: 0;
        justify-content: center;
        align-items: center;
        block-size: 32px;
        inline-size: 32px;
        border-radius: 9px;
        background: var(--primary-container);
        color: var(--on-primary-container);
      }
    }

    /* A two-line name/path stack that truncates rather than overflowing. */
    .ident {
      display: flex;
      flex-direction: column;
      min-inline-size: 0;

      .head-name {
        overflow: hidden;
        font-family: var(--font-mono);
        font-weight: 700;
        font-size: 14px;
        text-overflow: ellipsis;
        white-space: nowrap;
      }

      .head-path {
        overflow: hidden;
        color: var(--on-surface-var);
        font-family: var(--font-mono);
        font-size: 10px;
        text-overflow: ellipsis;
        white-space: nowrap;
      }
    }

    .item {
      display: flex;
      gap: 10px;
      align-items: center;
      inline-size: 100%;
      padding-block: 8px;
      padding-inline: 10px;
      border: none;
      border-radius: var(--r-sm);
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
        flex-shrink: 0;
        color: var(--on-surface-var);

        &.accent {
          color: var(--primary);
        }

        &.tertiary {
          color: var(--tertiary);
        }
      }

      .trail {
        display: inline-flex;
        flex-shrink: 0;
        color: var(--on-surface-var);
        opacity: 70%;
      }

      /* Icons/hints adopt the item's text colour on hover for a clean fill. */
      &:hover .lead,
      &:focus-visible .lead,
      &:hover .trail,
      &:focus-visible .trail {
        color: inherit;
      }

      .kbd {
        padding-block: 2px;
        padding-inline: 6px;
        border-radius: 6px;
        background: var(--surface-3);
        color: var(--on-surface-var);
        font-family: var(--font-mono);
        font-size: 10px;
      }

      &:hover .kbd,
      &:focus-visible .kbd {
        color: inherit;
      }
    }

    .item.recent {
      padding-block: 7px;
    }

    .recent-name {
      display: flex;
      gap: 6px;
      align-items: center;
    }

    .item.browse {
      color: var(--primary);

      &:hover,
      &:focus-visible {
        color: var(--on-primary-container);
      }
    }

    .sep {
      block-size: 1px;
      margin-block: 6px;
      margin-inline: 8px;
      background: var(--outline);
    }

    .eyebrow {
      color: var(--on-surface-var);
      font-weight: 700;
      font-size: 10px;
      letter-spacing: 0.08em;
      text-transform: uppercase;

      &.row {
        padding-block: 6px 4px;
        padding-inline: 10px;
      }

      &.spread {
        display: flex;
        gap: 8px;
        justify-content: space-between;
        align-items: center;
      }

      .count {
        font-weight: 600;
        font-variant-numeric: tabular-nums;
      }
    }

    .search {
      display: flex;
      gap: 8px;
      align-items: center;
      margin-block: 0 6px;
      margin-inline: 6px;
      padding-block: 6px;
      padding-inline: 10px;
      border: 1px solid var(--outline);
      border-radius: var(--r-sm);
      background: var(--surface-1);

      .lead {
        display: inline-flex;
        flex-shrink: 0;
        color: var(--on-surface-var);
      }

      input {
        flex: 1;
        min-inline-size: 0;
        border: none;
        background: transparent;
        color: var(--on-surface);
        outline: none;
        font: inherit;
        font-size: 13px;
      }
    }

    .recents {
      display: flex;
      flex-direction: column;
      gap: 2px;
      overflow-y: auto;
      max-block-size: 216px;
      padding: 2px;

      .empty {
        padding-block: 14px;
        padding-inline: 10px;
        color: var(--on-surface-var);
        font-size: 12px;
        text-align: center;
      }
    }
  }
</style>
