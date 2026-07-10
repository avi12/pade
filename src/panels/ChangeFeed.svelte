<script lang="ts">
  import { feed, ide, vcs } from "@/lib/bridge";
  import ColorText from "@/lib/ColorText.svelte";
  import { DiffKind, parseDiff, toSplitRows } from "@/lib/diff";
  import type { DiffLine, SplitRow } from "@/lib/diff";
  import { formatCount } from "@/lib/format";
  import Icon from "@/lib/Icon.svelte";
  import { effective } from "@/lib/prefs.svelte";
  import { setPanelHeader } from "@/lib/stores/sidePanel.svelte";
  import type { ChangeEvent, Ide } from "@/lib/types";
  import type { UnlistenFn } from "@tauri-apps/api/event";
  import { onDestroy, onMount } from "svelte";
  import { SvelteMap, SvelteSet } from "svelte/reactivity";

  // Newest first. Capped so a busy agent session can't grow this unbounded.
  let events = $state<ChangeEvent[]>([]);
  const CAP = 300;
  let unlisten: UnlistenFn | undefined;

  // Detected editors — the first is used to open a file from the diff title bar.
  let ides = $state<Ide[]>([]);

  // Inline diff viewer: only one card expands at a time.
  const DiffMode = {
    unified: "unified",
    split: "split"
  } as const;
  type DiffMode = (typeof DiffMode)[keyof typeof DiffMode];

  let expandedId = $state<string | null>(null);
  // Seed the viewer from the saved preference; the two enums share values.
  let diffMode = $state<DiffMode>(effective.diffStyle);
  // Cache raw parsed lines per event id so re-opening a card never refetches.
  // Keyed by id (not path) so repeated edits to the same file — distinct events
  // sharing a path — each render their own diff rather than the stale first one.
  const diffCache = new SvelteMap<string, DiffLine[]>();
  let loadingId = $state<string | null>(null);
  // Ids whose diff fetch failed, so a re-opened previously-failed card shows
  // "Couldn't load" rather than the empty-cache "No preview" message.
  const failedIds = new SvelteSet<string>();

  // A reactive clock so relative timestamps ("3m ago") tick forward on their own.
  let now = $state(Date.now());
  let clock: ReturnType<typeof setInterval> | undefined;

  onMount(async () => {
    clock = setInterval(() => {
      now = Date.now();
    }, 1000);
    unlisten = await feed.onChange(event => {
      events = [event, ...events].slice(0, CAP);
    });
    try {
      ides = await ide.suggest();
    } catch {
      ides = [];
    }
    // Watch the project the ADE was opened on.
    await feed.start();
  });

  onDestroy(() => {
    unlisten?.();

    if (clock !== undefined) {
      clearInterval(clock);
    }
  });

  const expandedEvent = $derived(events.find(item => item.id === expandedId) ?? null);
  const cachedLines = $derived(expandedEvent ? diffCache.get(expandedEvent.id) : undefined);
  const isLoading = $derived(!!expandedEvent && loadingId === expandedEvent.id);
  const isErrored = $derived(!!expandedEvent && failedIds.has(expandedEvent.id));
  const unifiedLines = $derived(cachedLines ?? []);
  const splitRows = $derived<SplitRow[]>(cachedLines ? toSplitRows(cachedLines) : []);
  const hasPreview = $derived(unifiedLines.length > 0);

  // Publish the live event count to the shared side-panel header.
  $effect(() => {
    setPanelHeader({
      count: events.length,
      refresh: null
    });
  });

  async function toggle(event: ChangeEvent) {
    const isAlreadyOpen = expandedId === event.id;
    if (isAlreadyOpen) {
      expandedId = null;
      return;
    }

    expandedId = event.id;

    if (diffCache.has(event.id)) {
      return;
    }

    loadingId = event.id;
    try {
      const raw = await vcs.diff({ path: event.path });
      diffCache.set(event.id, parseDiff(raw));
      failedIds.delete(event.id);
    } catch {
      failedIds.add(event.id);
      diffCache.set(event.id, []);
    } finally {
      loadingId = null;
    }
  }

  function openInEditor(path: string) {
    const editor = ides[0];
    if (editor) {
      void ide.open({
        command: editor.command,
        path
      });
    }
  }

  // Clicking the diff body reveals the file in the detected editor — a larger
  // target than the filename button. A drag to select text (for send-to-agent)
  // must not also open the file, so bail while a selection is live.
  const revealTip = $derived(ides[0] ? `Reveal in ${ides[0].label}` : "No editor detected");
  function revealDiff(path: string) {
    const selection = window.getSelection();
    if (selection && !selection.isCollapsed) {
      return;
    }

    openInEditor(path);
  }
  function onDiffKey({ event, path }: {
    event: KeyboardEvent;
    path: string;
  }) {
    if (event.key === "Enter") {
      openInEditor(path);
    }
  }

  function fileName(path: string) {
    return path.split(/[\\/]/).pop() ?? path;
  }
  function dir(path: string) {
    const parts = path.split(/[\\/]/);
    parts.pop();
    return parts.join("/");
  }
  function ago({ stamp, now }: {
    stamp: number;
    now: number;
  }) {
    const seconds = Math.max(0, Math.round((now - stamp) / 1000));
    if (seconds < 60) {
      return `${seconds}s ago`;
    }

    if (seconds < 3600) {
      return `${Math.round(seconds / 60)}m ago`;
    }

    return `${Math.round(seconds / 3600)}h ago`;
  }
</script>

<div class="feed">
  {#if events.length === 0}
    <p class="empty">
      Waiting for edits. Ask the agent to change a file and it appears here —
      what changed, and how much.
    </p>
  {/if}

  <ul class="cards">
    {#each events as ev (ev.id)}
      {@const isOpen = expandedId === ev.id}
      <li class="card {ev.kind}" class:open={isOpen}>
        <span class="stripe" aria-hidden="true"></span>
        <button class="body" aria-expanded={isOpen} onclick={() => void toggle(ev)}>
          <span class="row">
            <span class="dot {ev.kind}" aria-hidden="true"></span>
            <span class="name" data-tooltip={ev.path}>{fileName(ev.path)}</span>
            <span class="time">{ago({
              stamp: ev.ts,
              now
            })}</span>
          </span>
          <span class="summary">{ev.summary}</span>
          <span class="meta">
            <span class="path">{dir(ev.path)}</span>
            <span class="stat">
              {#if ev.added}
                <span class="add">+{formatCount(ev.added)}</span>
              {/if}
              {#if ev.removed}
                <span class="del">−{formatCount(ev.removed)}</span>
              {/if}
            </span>
          </span>
        </button>

        {#if isOpen}
          <div class="diff">
            <div class="bar">
              <button
                class="filebtn"
                data-tooltip={ides[0] ? `Open in ${ides[0].label}` : "No editor detected"}
                disabled={!ides[0]}
                onclick={() => openInEditor(ev.path)}
              >
                <Icon name="external" size={14} />
                <span class="fpath">{ev.path}</span>
              </button>
              <span class="spacer"></span>
              <div class="seg" aria-label="Diff view" role="group">
                <button
                  class:on={diffMode === DiffMode.unified}
                  aria-pressed={diffMode === DiffMode.unified}
                  onclick={() => (diffMode = DiffMode.unified)}
                >Unified</button>
                <button
                  class:on={diffMode === DiffMode.split}
                  aria-pressed={diffMode === DiffMode.split}
                  onclick={() => (diffMode = DiffMode.split)}
                >Split</button>
              </div>
              <button
                class="close"
                aria-label="Close diff"
                data-tooltip="Close"
                onclick={() => (expandedId = null)}
              >
                <Icon name="close" size={14} />
              </button>
            </div>

            {#if isLoading}
              <p class="state">Loading diff…</p>
            {:else if !hasPreview}
              {#if isErrored}
                <p class="state">Couldn't load a preview.</p>
              {:else}
                <p class="state">No preview available.</p>
              {/if}
            {:else if diffMode === DiffMode.unified}
              <div
                class="unified"
                data-tooltip={revealTip}
                onclick={() => revealDiff(ev.path)}
                onkeydown={event => onDiffKey({
                  event,
                  path: ev.path
                })}
                role="button"
                tabindex="0"
              >
                {#each unifiedLines as line, i (i)}
                  <div
                    class="line"
                    class:add={line.kind === DiffKind.add}
                    class:del={line.kind === DiffKind.del}
                    class:metaline={line.kind === DiffKind.meta}
                  ><ColorText text={line.text} /></div>
                {/each}
              </div>
            {:else}
              <div
                class="split"
                data-tooltip={revealTip}
                onclick={() => revealDiff(ev.path)}
                onkeydown={event => onDiffKey({
                  event,
                  path: ev.path
                })}
                role="button"
                tabindex="0"
              >
                {#each splitRows as row, i (i)}
                  {#if row.hunk}
                    <div class="hunk">{row.hunkText}</div>
                  {:else}
                    <div class="cell" class:filled-del={row.leftFilled}><ColorText text={row.left} /></div>
                    <div class="cell right" class:filled-add={row.rightFilled}><ColorText text={row.right} /></div>
                  {/if}
                {/each}
              </div>
            {/if}
          </div>
        {/if}
      </li>
    {/each}
  </ul>
</div>

<style>
  .feed {
    display: flex;
    flex-direction: column;
    height: 100%;
  }

  .empty {
    margin: 16px;
    color: var(--on-surface-variant);
    font-size: 13px;
    line-height: 1.5;
  }

  .cards {
    display: flex;
    flex: 1;
    flex-direction: column;
    gap: 8px;
    overflow-y: auto;
    min-block-size: 0;
    margin: 0;
    padding: 10px;
    list-style: none;
  }

  .card {
    position: relative;
    overflow: hidden;

    /* Border reserves its space always so the layout doesn't shift when a card
       opens; it's transparent when idle and lights to primary while expanded. */
    border: 1px solid transparent;
    border-radius: var(--radius-medium);
    background: var(--surface-1);
    transition:
      background 140ms var(--ease),
      border-color 140ms var(--ease);
    animation: pop-in 260ms var(--spring);

    &:hover {
      background: var(--surface-2);
    }

    &.open {
      border-color: var(--primary);
      background: var(--surface-2);
    }

    /* Accent stripe hugging the rounded left edge, tinted by change kind. */
    .stripe {
      position: absolute;
      inset-block: 0;
      inset-inline-start: 0;
      inline-size: 3px;
      background: var(--outline);
    }

    &.created .stripe {
      background: var(--tertiary);
    }

    &.modified .stripe {
      background: var(--primary);
    }

    &.deleted .stripe {
      background: var(--critical);
    }
  }

  /* The whole card content is one big toggle button; keep it text-aligned like a
     block, not a centred control. */
  .body {
    display: block;
    inline-size: 100%;
    padding-block: 11px;
    padding-inline: 15px 13px;
    text-align: start;
    cursor: pointer;
  }

  .row {
    display: flex;
    gap: 8px;
    align-items: center;
  }

  .dot {
    flex: none;
    block-size: 7px;
    inline-size: 7px;
    border-radius: 999px;
  }

  .dot.created {
    background: var(--tertiary);
  }

  .dot.modified {
    background: var(--primary);
  }

  .dot.deleted {
    background: var(--critical);
  }

  .name {
    overflow: hidden;
    font-family: var(--font-monospace);
    font-weight: 600;
    font-size: 13px;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .time {
    margin-left: auto;
    color: var(--on-surface-variant);
    font-size: 11px;
  }

  .summary {
    display: block;
    margin-block: 5px 0;
    margin-inline: 0;
    color: var(--on-surface);
    font-size: 13px;
  }

  .meta {
    display: flex;
    gap: 10px;
    align-items: center;
    margin-block-start: 6px;
    color: var(--on-surface-variant);
    font-size: 11px;
  }

  .path {
    overflow: hidden;
    font-family: var(--font-monospace);
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .stat {
    display: flex;
    gap: 8px;
    margin-inline-start: auto;
    font-weight: 600;
    font-variant-numeric: tabular-nums;
  }

  .add {
    color: var(--tertiary);
  }

  .del {
    color: var(--critical);
  }

  /* Inline diff viewer -------------------------------------------------- */
  .diff {
    overflow: hidden;
    margin-block: 0 11px;
    margin-inline: 15px 13px;
    border: 1px solid var(--outline);
    border-radius: var(--radius-medium);
    animation: rise 220ms var(--ease);
  }

  .bar {
    display: flex;
    gap: 8px;
    align-items: center;
    padding-block: 7px;
    padding-inline: 12px 8px;
    background: var(--surface-2);
  }

  .filebtn {
    display: inline-flex;
    gap: 6px;
    align-items: center;
    overflow: hidden;
    min-inline-size: 0;
    color: var(--primary);
    font-family: var(--font-monospace);
    font-weight: 600;
    font-size: 12px;
    white-space: nowrap;
    cursor: pointer;

    &:hover:not(:disabled) .fpath {
      text-decoration: underline;
    }

    &:disabled {
      color: var(--on-surface-variant);
      cursor: default;
    }
  }

  .fpath {
    overflow: hidden;
    text-overflow: ellipsis;
  }

  .spacer {
    flex: 1;
  }

  .seg {
    display: flex;
    flex-shrink: 0;
    gap: 2px;
    padding: 3px;
    border-radius: 999px;
    background: var(--surface-3);

    button {
      padding: 4px 11px;
      border-radius: 999px;
      color: var(--on-surface-variant);
      font-weight: 600;
      font-size: 11px;
      cursor: pointer;
      transition: background 120ms var(--ease);
    }

    .on {
      background: var(--primary-container);
      color: var(--on-primary-container);
    }
  }

  .close {
    display: inline-flex;
    flex-shrink: 0;
    justify-content: center;
    align-items: center;
    block-size: 26px;
    inline-size: 26px;
    border-radius: 999px;
    color: var(--on-surface-variant);
    cursor: pointer;

    &:hover {
      background: var(--surface-3);
      color: var(--on-surface);
    }
  }

  .state {
    margin: 0;
    padding: 14px 12px;
    background: var(--code-background);
    color: var(--on-surface-variant);
    font-size: 12px;
  }

  .unified {
    overflow: auto;
    max-block-size: 300px;
    padding-block: 8px;
    background: var(--code-background);
    font-family: var(--font-monospace);
    font-size: 12px;
    line-height: 1.5;
    cursor: pointer;

    .line {
      padding-inline: 12px;
      color: var(--code-foreground);
      white-space: pre;
    }

    .line.add {
      background: var(--tertiary-wash);
    }

    .line.del {
      background: var(--critical-wash);
    }

    .line.metaline {
      color: var(--on-surface-variant);
    }
  }

  .split {
    display: grid;
    grid-template-columns: 1fr 1fr;
    overflow: auto;
    max-block-size: 300px;
    padding-block: 8px;
    background: var(--code-background);
    font-family: var(--font-monospace);
    font-size: 12px;
    line-height: 1.5;
    cursor: pointer;

    .hunk {
      grid-column: 1 / -1;
      padding-inline: 12px;
      color: var(--on-surface-variant);
      white-space: pre;
    }

    .cell {
      overflow: hidden;
      min-block-size: 1.5em;
      padding-inline: 10px;
      border-inline-end: 1px solid var(--outline);
      color: var(--code-foreground);
      white-space: pre;
    }

    .cell.right {
      border-inline-end: none;
    }

    .cell.filled-del {
      background: var(--critical-wash);
    }

    .cell.filled-add {
      background: var(--tertiary-wash);
    }
  }
</style>
