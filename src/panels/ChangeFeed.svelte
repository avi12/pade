<script lang="ts">
  import { feed, ide } from "@/lib/bridge";
  import { firstChangedLine, parseDiff, unifiedDiff } from "@/lib/diff";
  import type { DiffLine } from "@/lib/diff";
  import DiffView from "@/lib/DiffView.svelte";
  import { formatCount } from "@/lib/format";
  import Icon from "@/lib/Icon.svelte";
  import { revealBlock } from "@/lib/motion";
  import { baseName, parentDir } from "@/lib/paths";
  import { effective } from "@/lib/prefs.svelte";
  import { setPanelHeader } from "@/lib/stores/sidePanel.svelte";
  import type { ChangeEvent, Ide } from "@/lib/types";
  import type { UnlistenFn } from "@tauri-apps/api/event";
  import { onDestroy, onMount, tick } from "svelte";
  import { SvelteMap, SvelteSet } from "svelte/reactivity";

  // The open project's root dir — lets an editor open a change in the window
  // that already has this project, at the right line.
  const { project }: { project: string } = $props();

  // Newest first. Capped so a busy agent session can't grow this unbounded.
  let events = $state<ChangeEvent[]>([]);
  const CAP = 300;
  let unlisten: UnlistenFn | undefined;

  // Editor/tool scratch files that churn during an atomic save (write to a temp
  // name, then rename over the target) — noise, not real changes. Match the
  // shapes the feed sees: a `.tmp.` infix, a `_tmp_` scratch name, a vim swap, a
  // trailing `~` backup, or a long numeric atomic-save suffix.
  const TEMP_FILE = /^_tmp_|\.tmp\.|\.sw[a-z]$|~$|\.\d{7,}$/i;

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

  // Swap the diff layout through a View Transition so unified↔split cross-fades in
  // place rather than snapping. Falls back to a plain swap when the API is absent
  // or the user prefers reduced motion. `tick()` lets Svelte apply the new layout
  // inside the transition callback so the API captures the correct "after" state.
  function setDiffMode(mode: DiffMode) {
    if (mode === diffMode) {
      return;
    }

    const reduceMotion = matchMedia("(prefers-reduced-motion: reduce)").matches;
    if (reduceMotion || !document.startViewTransition) {
      diffMode = mode;
      return;
    }

    document.startViewTransition(async () => {
      diffMode = mode;
      await tick();
    });
  }
  // Cache raw parsed lines per event id so re-opening a card never refetches.
  // Keyed by id (not path) so repeated edits to the same file — distinct events
  // sharing a path — each render their own diff rather than the stale first one.
  const diffCache = new SvelteMap<string, DiffLine[]>();
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
      const isScratchFile = TEMP_FILE.test(baseName(event.path));
      if (isScratchFile) {
        return;
      }

      events = [event, ...events].slice(0, CAP);
    });
    try {
      ides = await ide.suggest(project);
    } catch {
      ides = [];
    }
  });

  onDestroy(() => {
    unlisten?.();

    if (clock !== undefined) {
      clearInterval(clock);
    }
  });

  // Watch the open project — and re-root when this window switches projects. The
  // panel persists across a switch (only `project` updates, it never re-mounts),
  // so an effect keyed on `project` re-arms the watcher on every change, keeping
  // the feed on the workspace shown rather than the process's (possibly drifted)
  // cwd. Idempotent in the backend, so the initial mount's call is a no-op repeat.
  $effect(() => {
    if (project) {
      void feed.start(project);
    }
  });

  const expandedEvent = $derived(events.find(item => item.id === expandedId) ?? null);
  const cachedLines = $derived(expandedEvent ? diffCache.get(expandedEvent.id) : undefined);
  const isErrored = $derived(!!expandedEvent && failedIds.has(expandedEvent.id));
  const unifiedLines = $derived(cachedLines ?? []);
  const hasPreview = $derived(unifiedLines.length > 0);

  // Publish the live event count to the shared side-panel header.
  $effect(() => {
    setPanelHeader({
      count: events.length,
      refresh: null
    });
  });

  function openInEditor({ path, line }: {
    path: string;
    line?: number;
  }) {
    const editor = ides[0];
    if (editor) {
      void ide.openFile({
        command: editor.command,
        project,
        file: path,
        line
      });
    }
  }

  // Clicking the diff body (or the filename) opens the file in the selected
  // editor, jumped to the first changed line. The launcher hands the file to the
  // already-open editor when one is running, so it navigates there in place.
  const revealTip = $derived(ides[0] ? `Reveal in ${ides[0].label}` : "No editor detected");
  const revealLine = $derived(firstChangedLine(unifiedLines));
  function revealDiff({ path, event }: {
    path: string;
    event: MouseEvent;
  }) {
    // A drag to select text (for send-to-agent) must not also open the file.
    const selection = getSelection();
    if (selection && !selection.isCollapsed) {
      return;
    }

    // Open at the clicked line when a line was hit; else the first changed line.
    const target = event.target;
    const lineElement =
      target instanceof Element ? target.closest<HTMLElement>("[data-newline]") : null;
    const line = lineElement ? Number(lineElement.dataset.newline) : revealLine;
    openInEditor({
      path,
      line
    });
  }
  function onDiffKey({ event, path }: {
    event: KeyboardEvent;
    path: string;
  }) {
    if (event.key === "Enter") {
      openInEditor({
        path,
        line: revealLine
      });
    }
  }

  function ago({ stamp, now }: {
    stamp: number;
    now: number;
  }) {
    const seconds = Math.max(0, Math.round((now - stamp) / 1000));
    if (seconds < 60) {
      return `${formatCount(seconds)}s ago`;
    }

    if (seconds < 3600) {
      return `${formatCount(Math.round(seconds / 60))}m ago`;
    }

    return `${formatCount(Math.round(seconds / 3600))}h ago`;
  }
</script>

<div class="feed">
  {#if events.length === 0}
    <p class="empty">
      Waiting for edits. Ask the agent to change a file and it appears here —
      what changed, and how much.
    </p>
  {/if}

  <ul class="cards scroll-fade">
    {#each events as ev (ev.id)}
      {@const isOpen = expandedId === ev.id}
      <li class="card {ev.kind}" class:open={isOpen}>
        <span class="stripe" aria-hidden="true"></span>
        <button
          class="body"
          aria-expanded={isOpen}
          onclick={async () => {
            const isAlreadyOpen = expandedId === ev.id;
            if (isAlreadyOpen) {
              expandedId = null;
              return;
            }

            // Load the diff BEFORE expanding so the reveal measures the card's
            // full height and glides straight to it. Expanding first would animate
            // to the pre-diff height, then jump when the async diff lands.
            if (!diffCache.has(ev.id)) {
              try {
                // Git-free preview: the backend hands over the session baseline and
                // the current content; the shared parse+render path draws the diff.
                const preview = await feed.diff({ path: ev.path });
                const lines = preview
                  ? parseDiff(
                    unifiedDiff({
                      before: preview.before,
                      after: preview.after
                    })
                  )
                  : [];
                diffCache.set(ev.id, lines);
                failedIds.delete(ev.id);
              } catch {
                failedIds.add(ev.id);
                diffCache.set(ev.id, []);
              }
            }

            expandedId = ev.id;
          }}
        >
          <span class="row">
            <span class="dot {ev.kind}" aria-hidden="true"></span>
            <span class="name" data-tooltip={ev.path}>{baseName(ev.path)}</span>
            <span class="time">{ago({
              stamp: ev.ts,
              now
            })}</span>
          </span>
          <span class="summary">{ev.summary}</span>
          <span class="meta">
            <span class="path">{parentDir(ev.path) ?? ev.path}</span>
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
          <div class="diff" transition:revealBlock>
            <div class="bar">
              <button
                class="filebtn"
                data-tooltip={ides[0] ? `Open in ${ides[0].label}` : "No editor detected"}
                disabled={!ides[0]}
                onclick={() => openInEditor({
                  path: ev.path,
                  line: revealLine
                })}
              >
                <Icon name="external" size={14} />
                <span class="fpath">{ev.path}</span>
              </button>
              <span class="spacer"></span>
              {#if hasPreview}
                <div class="seg" aria-label="Diff view" role="group">
                  <button
                    class:on={diffMode === DiffMode.unified}
                    aria-pressed={diffMode === DiffMode.unified}
                    onclick={() => setDiffMode(DiffMode.unified)}
                  >Unified</button>
                  <button
                    class:on={diffMode === DiffMode.split}
                    aria-pressed={diffMode === DiffMode.split}
                    onclick={() => setDiffMode(DiffMode.split)}
                  >Split</button>
                </div>
              {/if}
              <button
                class="close"
                aria-label="Close diff"
                data-tooltip="Close"
                onclick={() => (expandedId = null)}
              >
                <Icon name="close" size={14} />
              </button>
            </div>

            {#if !hasPreview}
              {#if isErrored}
                <p class="state">Couldn't load a preview.</p>
              {:else}
                <p class="state">No preview available.</p>
              {/if}
            {:else}
              <div
                class="preview"
                data-tooltip={revealTip}
                onclick={e => revealDiff({
                  path: ev.path,
                  event: e
                })}
                onkeydown={e => onDiffKey({
                  event: e,
                  path: ev.path
                })}
                role="button"
                tabindex="0"
              >
                <DiffView diffLines={unifiedLines} split={diffMode === DiffMode.split} />
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
    gap: 10px;
    overflow-y: auto;
    overscroll-behavior: contain;
    min-block-size: 0;
    margin: 0;
    padding: 10px;
    list-style: none;
  }

  .card {
    contain-intrinsic-block-size: auto 86px;
    position: relative;

    /* Offscreen cards skip layout and paint entirely, so a full feed (the
       300-event cap) scrolls without jank; the placeholder size keeps the
       scrollbar honest until a card has rendered once and remembers its own. */
    content-visibility: auto;

    /* Never let the scroller's flex column squash a card to fit — an
       overflowing feed scrolls; cards keep their natural height. */
    flex-shrink: 0;
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
    font-variant-numeric: tabular-nums;
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

  /* Scroll container around the shared DiffView; the click-to-reveal cursor
     and cap live here, the line rendering in DiffView. */
  .preview {
    overflow: auto;
    max-block-size: 300px;
    padding-block: 8px;
    background: var(--code-background);
    cursor: pointer;

    /* Scopes the unified↔split View Transition to the diff body (only one card
       is ever open, so the name is unique) — the rest of the app stays still. */
    view-transition-name: diff-body;
  }
</style>
