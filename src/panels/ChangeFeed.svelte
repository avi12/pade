<script lang="ts">
  import { feed, ide, members, vcs } from "@/lib/bridge";
  import { groupChanges, GroupRole } from "@/lib/change-groups";
  import { firstChangedLine, parseDiff, unifiedDiff } from "@/lib/diff";
  import type { DiffLine } from "@/lib/diff";
  import DiffView from "@/lib/DiffView.svelte";
  import { fileExtension, fileTypeBadge } from "@/lib/file-type";
  import { formatCount, formatTimestamp } from "@/lib/format";
  import Icon from "@/lib/Icon.svelte";
  import { markdownDocument, sandboxedHtmlDocument } from "@/lib/markdown";
  import {
    collapseRow,
    emphasized,
    expandRow,
    flipDuration,
    revealBlock
  } from "@/lib/motion";
  import { baseName, parentDir } from "@/lib/paths";
  import { effective } from "@/lib/prefs.svelte";
  import { isHtmlPath, isImagePath, isMarkdownPath } from "@/lib/preview";
  import { editorsFor, ensureEditors } from "@/lib/stores/editors.svelte";
  import { feedStore, retarget } from "@/lib/stores/feed.svelte";
  import { setPanelHeader } from "@/lib/stores/sidePanel.svelte";
  import { showToast } from "@/lib/stores/toast.svelte";
  import { TooltipAttribute, truncationTooltip } from "@/lib/truncation-tooltip";
  import type { FeedDiff, WorkspaceMember } from "@/lib/types";
  import type { UnlistenFn } from "@tauri-apps/api/event";
  import { onDestroy, onMount, tick } from "svelte";
  import { flip } from "svelte/animate";
  import { SvelteMap, SvelteSet } from "svelte/reactivity";

  // The open project's root dir — lets an editor open a change in the window
  // that already has this project, at the right line.
  const { project }: { project: string } = $props();

  // The feed events live in a persistent store (lib/stores/feed) that owns the one
  // live subscription, so switching side panels away and back keeps the feed —
  // this component only reads and renders them.

  // The project's ranked editors, read from the shared store (SSOT — the same
  // list the top-bar IdeMenu shows, so a reveal here and the launcher there can
  // never name different editors). The reveal uses the first *windowed* editor:
  // a console editor (Neovim/Vim/Helix) can't run detached — handing it to the
  // OS launcher spawns an invisible orphan that locks the workspace cwd — and
  // the feed has no terminal tab to route it into, so it's skipped here.
  const ides = $derived(editorsFor(project));
  const revealEditor = $derived(ides.find(editor => !editor.terminal));

  // The workspace's current git branch (all groups share the one repo/HEAD), for
  // the group-header subtitle. Empty for a non-repo / detached-HEAD workspace.
  let branchByPath = $state<Record<string, string>>({});
  const branch = $derived(branchByPath[project]);

  async function loadBranch(root: string) {
    try {
      branchByPath = await vcs.branchOf([root]);
    } catch {
      branchByPath = {};
    }
  }

  // "Sync all" only makes sense when there's a remote to fast-forward from — a
  // fresh local project (git-init'd but no origin) or a non-repo has nothing to
  // pull. `remoteUrl` is null with no remote and rejects outside a repo, so the
  // button stays hidden in both cases and appears only for a repo with a remote.
  async function loadRemote(root: string) {
    try {
      const remote = await vcs.remoteUrl(root);
      if (root === project) {
        hasRemote = remote !== null;
      }
    } catch {
      if (root === project) {
        hasRemote = false;
      }
    }
  }

  // Manifest-confirmed workspace members (backend census) — the grouping ground
  // truth change-groups prefers over folder-name conventions. Refetched on a
  // project switch; a failure just leaves the convention fallback in charge.
  let workspaceMembers = $state<WorkspaceMember[]>([]);

  async function loadMembers(root: string) {
    try {
      workspaceMembers = await members.list(root);
    } catch {
      workspaceMembers = [];
    }
  }

  // "Sync all" (fast-forward pull) in-flight guard — disables the button and
  // spins its icon so a slow fetch can't be double-fired.
  let syncing = $state(false);
  // Whether the workspace has a git remote to sync from — gates the whole button
  // (see loadRemote). A fresh local project or a non-repo has nothing to pull.
  let hasRemote = $state(false);

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
  // Rendered image previews per event id: the data URL for an image card, or
  // `null` when the backend has nothing to show (gone, or over its size cap).
  // Fetched lazily on first expand, exactly like the diff cache — a feed full of
  // image changes never fetches them all at once, only the card being opened.
  const imageCache = new SvelteMap<string, string | null>();
  // Ids whose diff fetch failed, so a re-opened previously-failed card shows
  // "Couldn't load" rather than the empty-cache "No preview" message.
  const failedIds = new SvelteSet<string>();

  // A markdown/HTML card shows a Code|Preview toggle: Code is the diff (the
  // feed's core purpose, so the default), Preview is the rendered result in a
  // sandboxed iframe. The chosen pane is kept per card id.
  const PreviewPane = {
    code: "code",
    preview: "preview"
  } as const;
  type PreviewPane = (typeof PreviewPane)[keyof typeof PreviewPane];
  const paneById = new SvelteMap<string, PreviewPane>();
  // The sandbox-ready srcdoc per card id (a full HTML document for markdown, the
  // raw file text for HTML), or `null` when its text couldn't be fetched.
  // Fetched lazily the first time a card switches to Preview — exactly like the
  // image cache, so a feed of docs never fetches them all at once.
  const previewDocCache = new SvelteMap<string, string | null>();

  function paneOf(id: string): PreviewPane {
    return paneById.get(id) ?? PreviewPane.code;
  }

  // The iframe srcdoc for a file's current text: markdown is rendered to a full
  // themed document, an HTML file is shown as-is (both inert — see the sandboxed
  // iframe below).
  function previewDocument({ path, text }: {
    path: string;
    text: string;
  }): string {
    return isMarkdownPath(path) ? markdownDocument(text) : sandboxedHtmlDocument(text);
  }

  // Switch a card to its Preview pane, fetching the file's current text on the
  // first switch and caching the srcdoc. Own error handling, so the promise
  // never rejects.
  async function showPreview({ id, path }: {
    id: string;
    path: string;
  }): Promise<void> {
    paneById.set(id, PreviewPane.preview);

    if (previewDocCache.has(id)) {
      return;
    }

    try {
      const text = await feed.text({ path });
      if (text === null) {
        previewDocCache.set(id, null);
        return;
      }

      previewDocCache.set(
        id, previewDocument({
          path,
          text
        })
      );
      failedIds.delete(id);
    } catch {
      failedIds.add(id);
      previewDocCache.set(id, null);
    }
  }

  // A reactive clock so relative timestamps ("3m ago") tick forward on their own.
  let now = $state(Date.now());
  let clock: ReturnType<typeof setInterval> | undefined;
  // The live git-state subscription — a branch switch, remote change, or git
  // init in the workspace re-reads the branch subtitle without a remount.
  let unlistenGitState: UnlistenFn | undefined;

  onMount(async () => {
    clock = setInterval(() => {
      // Only repaint when a label can actually change: per second while the
      // newest card still shows a seconds age ("41s ago"), else once a minute
      // (older labels have minute granularity). An idle feed repainting every
      // second is exactly the kind of background frame churn to avoid.
      const newest = feedStore.events[0]?.ts;
      const showsSecondsAge = newest !== undefined && Date.now() - newest < 60_000;
      const minuteRolledOver = Math.floor(Date.now() / 60_000) !== Math.floor(now / 60_000);
      if (showsSecondsAge || minuteRolledOver) {
        now = Date.now();
      }
    }, 1000);
    unlistenGitState = await vcs.onStateChanged(() => {
      loadBranch(project);
      loadRemote(project);
    });
  });

  onDestroy(() => {
    if (clock !== undefined) {
      clearInterval(clock);
    }

    unlistenGitState?.();
  });

  // The file-watcher itself is started and re-rooted app-wide by App (keyed on
  // the open project), so the feed just subscribes to its stream above — no need
  // to arm it here.

  // The workspace's git subtitle, remote gating, manifest members, and editors,
  // loaded together (concurrently) on mount and on a project switch. Each callee
  // owns its own error handling, so the gathered promise never rejects.
  async function loadWorkspaceState(workspace: string): Promise<void> {
    await Promise.all([
      loadBranch(workspace),
      loadRemote(workspace),
      loadMembers(workspace),
      // Editors come from the shared store's cache on a mere remount (a side-panel
      // switch); a fetch only runs when nothing is cached for this project yet.
      ensureEditors(workspace)
    ]);
  }

  // Re-read the workspace branch on mount and whenever the window switches
  // projects, and point the persistent feed at the current project so a workspace
  // switch clears stale events even if the panel was closed during the switch.
  // Reacts to `project` — the loads write state this effect doesn't read, and the
  // editors store's trigger functions are reactivity-transparent (its bookkeeping
  // maps are non-reactive on purpose), so this fires per workspace, never per tick.
  $effect(() => {
    const workspace = project;
    if (!workspace) {
      return;
    }

    retarget(workspace);
    loadWorkspaceState(workspace);
  });

  const expandedEvent = $derived(feedStore.events.find(item => item.id === expandedId) ?? null);
  const cachedLines = $derived(expandedEvent ? diffCache.get(expandedEvent.id) : undefined);
  const isErrored = $derived(!!expandedEvent && failedIds.has(expandedEvent.id));
  const unifiedLines = $derived(cachedLines ?? []);
  const hasPreview = $derived(unifiedLines.length > 0);

  // An open card is a view of one revision, not merely one path. When the agent
  // writes that same path again, the old card is now stale; follow the newest
  // event automatically so the expanded preview always shows the live change.
  $effect(() => {
    const open = expandedEvent;
    if (!open) {
      return;
    }

    const newestForPath = feedStore.events.find(event => event.path === open.path);
    if (newestForPath && newestForPath.id !== open.id) {
      expandedId = newestForPath.id;
    }
  });

  // ── Grouping + filters ──────────────────────────────────────────────────────
  // Bucket the feed by project (manifest members first, folder-name convention
  // as the fallback — see change-groups), and let the chip row narrow to one
  // project and the "File type" filter narrow to a set of file extensions.
  let activeGroupId = $state<string | null>(null);

  // A card's dir line ellipsizes in the narrow panel; only while actually
  // clipped does the bubble appear — and it carries the FULL path, since the
  // clipped parent dir alone is exactly what the reader couldn't see.
  function clippedPathTooltip(path: string) {
    return truncationTooltip({
      tooltip: path,
      attribute: TooltipAttribute.Bubble
    });
  }

  interface ExtensionCount {
    extension: string;
    count: number;
  }

  // The per-extension tally over the live feed — the one source both the filter
  // popover's counts and its filtering read (SSOT), computed once here rather
  // than re-derived at each call site. Sorted by count, then name, so the
  // busiest types lead and the order is stable.
  const extensionCounts = $derived.by<ExtensionCount[]>(() => {
    const counts: Record<string, number> = {};
    for (const event of feedStore.events) {
      const extension = fileExtension(event.path);
      counts[extension] = (counts[extension] ?? 0) + 1;
    }

    return Object.entries(counts)
      .map(([extension, count]) => ({
        extension,
        count
      }))
      .sort((first, second) => second.count - first.count || first.extension.localeCompare(second.extension));
  });

  // Opt-in file-type filter: the extensions the user has ticked. An empty set is
  // the default and means "no filter — show every type", so a checkbox is empty
  // by default and the feed stays fully visible until a type is picked.
  const selectedExtensions = new SvelteSet<string>();

  const allTypesSelected = $derived(
    extensionCounts.length > 0 && extensionCounts.every(item => selectedExtensions.has(item.extension))
  );
  const filterLabel = $derived.by(() => {
    const noFilterActive = selectedExtensions.size === 0 || allTypesSelected;
    if (noFilterActive) {
      return "All types";
    }

    return `${formatCount(selectedExtensions.size)} of ${formatCount(extensionCounts.length)} types`;
  });

  function toggleExtension(extension: string) {
    if (selectedExtensions.has(extension)) {
      selectedExtensions.delete(extension);
      return;
    }

    selectedExtensions.add(extension);
  }

  // The header text action: fills every box when not all are ticked, else clears
  // the selection back to the "show all" default.
  function toggleAllTypes() {
    if (allTypesSelected) {
      selectedExtensions.clear();
      return;
    }

    for (const item of extensionCounts) {
      selectedExtensions.add(item.extension);
    }
  }

  const typeFiltered = $derived.by(() => {
    if (selectedExtensions.size === 0) {
      return feedStore.events;
    }

    return feedStore.events.filter(event => selectedExtensions.has(fileExtension(event.path)));
  });
  const groups = $derived(
    groupChanges({
      events: typeFiltered,
      workspaceRoot: project,
      members: workspaceMembers
    })
  );
  // A chip selection can outlive its group (its events filtered out or aged past
  // the cap); when it does, fall back to showing every group.
  const selectionStillExists = $derived(
    activeGroupId !== null && groups.some(group => group.id === activeGroupId)
  );
  const visibleGroups = $derived.by(() => {
    if (activeGroupId === null || !selectionStillExists) {
      return groups;
    }

    return groups.filter(group => group.id === activeGroupId);
  });

  function roleLabel(role: GroupRole): string {
    if (role === GroupRole.Service) {
      return "SVC";
    }

    return role.toUpperCase();
  }

  // Publish the live event count to the shared side-panel header.
  $effect(() => {
    setPanelHeader({
      count: feedStore.events.length,
      refresh: null
    });
  });

  function openInEditor({ path, line }: {
    path: string;
    line?: number;
  }) {
    if (revealEditor) {
      ide.openFile({
        command: revealEditor.command,
        project,
        file: path,
        line
      });
    }
  }

  // Clicking the diff body (or the filename) opens the file in the selected
  // editor, jumped to the first changed line. The launcher hands the file to the
  // already-open editor when one is running, so it navigates there in place.
  const revealTip = $derived(revealEditor ? `Reveal in ${revealEditor.label}` : "No editor detected");
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

  // Turn the backend's session-baseline pair into diff lines to render. Normally
  // that's the baseline→current diff. But a file the watcher first saw a beat
  // AFTER it was written baselines equal to its current content — the write beat
  // the first watch poll — so that diff is empty even though the card counts real
  // growth ("Grew by N lines"). Rather than strand it on "No preview available",
  // fall back to previewing the whole current file: everything a file created this
  // session holds IS new, so the full content is the honest preview. A deletion
  // (empty `after`) keeps its real removal diff — no fallback.
  function previewLines(preview: FeedDiff | null): DiffLine[] {
    if (!preview) {
      return [];
    }

    const changed = parseDiff(
      unifiedDiff({
        before: preview.before,
        after: preview.after
      })
    );
    if (changed.length > 0 || preview.after.length === 0) {
      return changed;
    }

    return parseDiff(
      unifiedDiff({
        before: "",
        after: preview.after
      })
    );
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
  {#if feedStore.events.length === 0}
    <p class="empty">
      Waiting for edits. Ask the agent to change a file and it appears here —
      what changed, and how much.
    </p>
  {:else}
    <div class="toolbar">
      <div class="typefilter menu-host">
        <button
          style:anchor-name="--type-anchor"
          class="typechip menu-trigger"
          popovertarget="type-filter"
        >
          <span class="caret">▽</span>
          <span class="lbl">{filterLabel}</span>
        </button>

        <div id="type-filter" style:position-anchor="--type-anchor" class="type-popover popover-menu" popover>
          <div class="pop-head">
            <span class="pop-title">File type</span>
            <button class="selectall" onclick={() => toggleAllTypes()}>
              {#if allTypesSelected}
                Clear{:else}Select all{/if}
            </button>
          </div>
          <ul class="typelist">
            {#each extensionCounts as item (item.extension)}
              <li
                in:expandRow
                out:collapseRow
                animate:flip={{
                  duration: flipDuration(),
                  easing: emphasized
                }}
              >
                <label class="check typerow">
                  <span class="ck">
                    <input
                      checked={selectedExtensions.has(item.extension)}
                      onchange={() => toggleExtension(item.extension)}
                      type="checkbox"
                    />
                    <span class="box" aria-hidden="true">
                      <svg fill="none" viewBox="0 0 24 24"><path d="M5 12.5l4.5 4.5L19 7" /></svg>
                    </span>
                  </span>
                  <span class="ext">{item.extension}</span>
                  <span class="n">{formatCount(item.count)}</span>
                </label>
              </li>
            {/each}
          </ul>
        </div>
      </div>

      {#if hasRemote}
        <button
          class="sync"
          data-tooltip="Fast-forward this workspace from origin"
          disabled={syncing}
          onclick={async () => {
            syncing = true;
            try {
              const outcome = await vcs.pull(project);
              showToast(outcome.message);
            } catch (error) {
              const text = error instanceof Error ? error.message : String(error);
              showToast(text.split("\n")[0] || "Sync failed.");
            } finally {
              syncing = false;
            }
          }}
        >
          <span class="ico" class:spin={syncing}><Icon name="refresh" size={14} /></span>
          Sync all
        </button>
      {/if}
    </div>

    {#if groups.length > 1 || activeGroupId !== null}
      <div class="chips">
        <button class="chip" class:on={activeGroupId === null} onclick={() => (activeGroupId = null)}>
          All <span class="n">{formatCount(feedStore.events.length)}</span>
        </button>
        {#each groups as group (group.id)}
          <button
            class="chip"
            class:on={activeGroupId === group.id}
            onclick={() => (activeGroupId = group.id)}
          >{group.name} <span class="n">{formatCount(group.events.length)}</span></button>
        {/each}
      </div>
    {/if}

    <div class="cards scroll-fade">
      {#each visibleGroups as group (group.id)}
        <section class="group">
          <header class="ghead">
            <span class="badge {group.role}">{roleLabel(group.role)}</span>
            <span class="gname">{group.name}</span>
            {#if branch}
              <span class="gbranch"><Icon name="branch" size={12} />{branch}</span>
            {/if}
            <span class="gstat">
              {#if group.added}
                <span class="add">+{formatCount(group.added)}</span>
              {/if}
              {#if group.removed}
                <span class="del">−{formatCount(group.removed)}</span>
              {/if}
            </span>
            <span class="gcount">{formatCount(group.events.length)}</span>
          </header>
          <ul class="grouplist">
            {#each group.events as ev (ev.id)}
              {@const isOpen = expandedId === ev.id}
              {@const badge = fileTypeBadge(ev.path)}
              {@const isImage = isImagePath(ev.path)}
              {@const canPreview = isMarkdownPath(ev.path) || isHtmlPath(ev.path)}
              {@const pane = paneOf(ev.id)}
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

                    // An image card renders the picture, not a text diff — fetch its
                    // rendered preview (a data URL) instead of the baseline diff, also
                    // BEFORE expanding so the reveal measures the final height.
                    if (isImage) {
                      if (!imageCache.has(ev.id)) {
                        try {
                          const preview = await feed.image({ path: ev.path });
                          imageCache.set(ev.id, preview?.dataUrl ?? null);
                          failedIds.delete(ev.id);
                        } catch {
                          failedIds.add(ev.id);
                          imageCache.set(ev.id, null);
                        }
                      }

                      expandedId = ev.id;
                      return;
                    }

                    // Load the diff BEFORE expanding so the reveal measures the card's
                    // full height and glides straight to it. Expanding first would animate
                    // to the pre-diff height, then jump when the async diff lands.
                    if (!diffCache.has(ev.id)) {
                      try {
                        // Git-free preview: the backend hands over the session baseline and
                        // the current content; the shared parse+render path draws the diff
                        // (or the whole file when the baseline landed late — see previewLines).
                        const preview = await feed.diff({ path: ev.path });
                        diffCache.set(ev.id, previewLines(preview));
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
                    <span class="ftype tone-{badge.tone}" class:logo={!!badge.icon} aria-hidden="true">
                      {#if badge.icon}
                        <Icon name={badge.icon} size={18} />
                      {:else}
                        {badge.label}
                      {/if}
                    </span>
                    <span class="name" data-tooltip={ev.path}>{baseName(ev.path)}</span>
                    <span class="time" data-tooltip={formatTimestamp(ev.ts)}>{ago({
                      stamp: ev.ts,
                      now
                    })}</span>
                  </span>
                  <span class="summary">{ev.summary}</span>
                  <span class="meta">
                    <span class="path" {@attach clippedPathTooltip(ev.path)}>{parentDir(ev.path) ?? ev.path}</span>
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
                        data-tooltip={revealEditor ? `Open in ${revealEditor.label} · ${ev.path}` : ev.path}
                        disabled={!revealEditor}
                        onclick={() => openInEditor({
                          path: ev.path,
                          line: revealLine
                        })}
                      >
                        <Icon name="external" size={14} />
                        <!-- The path is usually clipped in this narrow panel, so the full
                             path (with the open-in action) rides in the tooltip — the shared
                             CSS `[data-tooltip]` bubble, which caps at 320px + wraps and
                             anchor-positions with a flip-up fallback. -->
                        <span class="fpath">{ev.path}</span>
                      </button>
                      <span class="spacer"></span>
                      {#if canPreview}
                        <div class="seg" aria-label="Preview mode" role="group">
                          <button
                            class:on={pane === PreviewPane.code}
                            aria-pressed={pane === PreviewPane.code}
                            onclick={() => paneById.set(ev.id, PreviewPane.code)}
                          >Code</button>
                          <button
                            class:on={pane === PreviewPane.preview}
                            aria-pressed={pane === PreviewPane.preview}
                            onclick={() => showPreview({
                              id: ev.id,
                              path: ev.path
                            })}
                          >Preview</button>
                        </div>
                      {/if}
                      {#if !isImage && pane === PreviewPane.code && hasPreview}
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

                    {#if isImage}
                      {@const imageUrl = imageCache.get(ev.id)}
                      {#if imageUrl}
                        <div class="imgwrap">
                          <img alt={baseName(ev.path)} loading="lazy" src={imageUrl} />
                        </div>
                      {:else if isErrored}
                        <p class="state">Couldn't load the image.</p>
                      {:else}
                        <p class="state">No image preview available.</p>
                      {/if}
                    {:else if canPreview && pane === PreviewPane.preview}
                      {@const doc = previewDocCache.get(ev.id)}
                      {#if doc}
                        <!-- The untrusted markdown/HTML render is quarantined in a
                             sandboxed iframe with NO allow-scripts and NO
                             allow-same-origin: it runs no scripts, has an opaque
                             origin, and can't reach the parent — the web analog of
                             PowerToys' locked-down WebView2. `srcdoc` keeps it fully
                             inline (no external ref under the CSP). -->
                        <iframe
                          class="render"
                          sandbox=""
                          srcdoc={doc}
                          title="Preview of {baseName(ev.path)}"
                        ></iframe>
                      {:else if previewDocCache.has(ev.id)}
                        {#if isErrored}
                          <p class="state">Couldn't render the preview.</p>
                        {:else}
                          <p class="state">No preview available.</p>
                        {/if}
                      {:else}
                        <p class="state">Rendering preview…</p>
                      {/if}
                    {:else if !hasPreview}
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
        </section>
      {/each}
    </div>
  {/if}
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

  .toolbar {
    display: flex;
    gap: 8px;
    align-items: center;
    padding-block: 10px 0;
    padding-inline: 10px;
  }

  /* File-type filter: a pill that reads the current selection and drops a native
     popover of per-extension checkboxes. The pressed state + caret flip come from
     the shared .menu-host/.menu-trigger chrome (theme.css). */
  .typefilter {
    display: inline-flex;
  }

  .typechip {
    display: inline-flex;
    gap: 6px;
    align-items: center;
    padding-block: 4px;
    padding-inline: 10px 12px;
    border: 1px solid var(--outline);
    border-radius: 999px;
    background: var(--surface-2);
    color: var(--on-surface);
    font-family: inherit;
    font-weight: 600;
    font-size: 12px;
    cursor: pointer;
    transition: background 120ms var(--ease);

    &:hover {
      background: var(--surface-3);
    }

    .caret {
      font-size: 10px;
      transition: rotate 200ms var(--ease);
    }

    .lbl {
      font-variant-numeric: tabular-nums;
    }
  }

  /* Anchored below the chip. Uses the shared .popover-menu shell for its flip
     fallbacks, but overrides the surface to a tonal surface-container card with
     the M3 large radius, softer elevation, and airier padding to match design. */
  .type-popover {
    overflow-y: auto;
    max-block-size: 60vh;
    min-inline-size: 224px;
    padding: 6px;
    border: none;
    border-radius: var(--radius-large);
    background: var(--surface-2);
    box-shadow: 0 8px 28px var(--shadow-color);
    position-area: bottom span-right;

    .pop-head {
      display: flex;
      gap: 12px;
      justify-content: space-between;
      align-items: center;
      padding-block: 8px 6px;
      padding-inline: 12px;
    }

    .pop-title {
      color: var(--on-surface-variant);
      font-weight: 700;
      font-size: 10px;
      letter-spacing: 0.08em;
      text-transform: uppercase;
    }

    /* "Select all" is a text action, not a checkbox: a plain blue link-button. */
    .selectall {
      padding: 0;
      border: none;
      background: none;
      color: var(--primary);
      font-family: inherit;
      font-weight: 600;
      font-size: 12px;
      cursor: pointer;

      &:hover {
        text-decoration: underline;
      }
    }

    .typelist {
      display: flex;
      flex-direction: column;
      gap: 2px;
      margin: 0;
      padding-block: 2px 0;
      padding-inline: 0;
      list-style: none;
    }

    .check {
      display: flex;
      gap: 10px;
      align-items: center;
      font-size: 13px;
      cursor: pointer;
    }

    /* Shrink the shared 20px .ck to a menu-fit 18px. */
    .ck {
      block-size: 18px;
      inline-size: 18px;

      .box {
        block-size: 18px;
        inline-size: 18px;
      }
    }

    .typerow {
      padding-block: 7px;
      padding-inline: 12px;
      border-radius: var(--radius-small);

      &:hover {
        background: var(--surface-3);
      }

      .ext {
        flex: 1;
        overflow: hidden;
        font-family: var(--font-monospace);
        text-overflow: ellipsis;
        white-space: nowrap;
      }

      .n {
        color: var(--on-surface-variant);
        font-variant-numeric: tabular-nums;
      }
    }
  }

  .sync {
    display: inline-flex;
    gap: 6px;
    align-items: center;
    margin-inline-start: auto;
    padding: 4px 12px;
    border: 1px solid var(--outline);
    border-radius: 999px;
    background: var(--surface-2);
    color: var(--on-surface);
    font-family: inherit;
    font-weight: 600;
    font-size: 12px;
    cursor: pointer;
    transition: background 120ms var(--ease);

    &:hover:not(:disabled) {
      background: var(--surface-3);
    }

    &:disabled {
      color: var(--on-surface-variant);
      cursor: default;
    }

    .ico {
      display: inline-flex;
      color: var(--primary);
    }

    /* Reuses the global `spin` keyframe (theme.css); the global
       reduced-motion reset disables it for those who ask. */
    .spin {
      animation: spin 900ms linear infinite;
    }
  }

  .chips {
    display: flex;
    flex-wrap: wrap;
    gap: 6px;
    padding-block: 10px 0;
    padding-inline: 10px;
  }

  .chip {
    display: inline-flex;
    gap: 6px;
    align-items: center;
    padding: 4px 11px;
    border: 1px solid var(--outline);
    border-radius: 999px;
    background: var(--surface-2);
    color: var(--on-surface-variant);
    font-size: 12px;
    cursor: pointer;
    transition:
      background 120ms var(--ease),
      color 120ms var(--ease);

    &:hover {
      background: var(--surface-3);
    }

    &.on {
      border-color: transparent;
      background: var(--primary-container);
      color: var(--on-primary-container);
      font-weight: 600;
    }

    .n {
      font-variant-numeric: tabular-nums;
      opacity: 75%;
    }
  }

  .cards {
    display: flex;
    flex: 1;
    flex-direction: column;
    gap: 18px;
    overflow-y: auto;
    overscroll-behavior: contain;
    min-block-size: 0;
    margin: 0;
    padding: 10px;
  }

  .group {
    display: flex;
    flex-direction: column;
  }

  .ghead {
    display: flex;
    gap: 9px;
    align-items: center;
    padding-block: 2px 8px;
    padding-inline: 6px;
  }

  .badge {
    flex: none;
    padding: 2px 6px;
    border-radius: var(--radius-small);
    color: #ffffff;
    font-family: var(--font-monospace);
    font-weight: 700;
    font-size: 9.5px;
    letter-spacing: 0.05em;

    &.app {
      background: #3b6fe0;
    }

    &.lib {
      background: #12a58a;
    }

    &.service {
      background: #c8871a;
    }
  }

  .gname {
    overflow: hidden;
    font-weight: 700;
    font-size: 13px;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  /* Git-branch subtitle beside the group name — leads with the branch glyph, to
     match the switcher's branch chip; muted so the project name stays primary. */
  .gbranch {
    display: inline-flex;
    flex: none;
    gap: 3px;
    align-items: center;
    color: var(--on-surface-variant);
    font-family: var(--font-monospace);
    font-size: 11px;
    font-variant-numeric: tabular-nums;
    white-space: nowrap;
  }

  .gstat {
    display: flex;
    gap: 8px;
    margin-inline-start: auto;
    font-family: var(--font-monospace);
    font-weight: 600;
    font-size: 11px;
    font-variant-numeric: tabular-nums;
  }

  .gcount {
    flex: none;
    min-inline-size: 20px;
    padding: 1px 7px;
    border-radius: 999px;
    background: var(--surface-2);
    color: var(--on-surface-variant);
    font-size: 10.5px;
    font-variant-numeric: tabular-nums;
    text-align: center;
  }

  .grouplist {
    display: flex;
    flex-direction: column;
    gap: 8px;
    margin: 0;
    padding: 0;
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

  .ftype {
    display: inline-grid;
    flex: none;
    place-items: center;
    block-size: 20px;
    min-inline-size: 30px;
    padding-inline: 5px;
    border-radius: 6px;
    background: var(--tone, #6b7280);
    color: #ffffff;
    font-family: var(--font-monospace);
    font-weight: 700;
    font-size: 9.5px;
    letter-spacing: 0.02em;

    /* Brand-logo variant: drop the text chip's pill and tint the mark with the
       language tone. Multi-colour marks carry their own baked fills (the tone is
       a no-op there); single-colour marks and the format glyphs take the tone.
       The Icon renders at 1em, so the font-size sets its square. */
    &.logo {
      inline-size: 20px;
      min-inline-size: 0;
      padding: 0;
      background: none;
      color: var(--tone, #6b7280);
      font-size: 18px;
    }
  }

  .tone-typescript {
    --tone: #3b6fe0;
  }

  .tone-javascript {
    --tone: #c9971a;
  }

  .tone-svelte {
    --tone: #e0701c;
  }

  .tone-rust {
    --tone: #c56a1a;
  }

  .tone-style {
    --tone: #2596be;
  }

  .tone-markup {
    --tone: #d9822b;
  }

  .tone-python {
    --tone: #3572a5;
  }

  .tone-go {
    --tone: #2ba7bd;
  }

  .tone-data {
    --tone: #7a8290;
  }

  .tone-doc {
    --tone: #6b7688;
  }

  .tone-shell {
    --tone: #55607a;
  }

  .tone-image {
    --tone: #a05fd6;
  }

  .tone-neutral {
    --tone: #6b7280;
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

  /* Rendered markdown/HTML preview: a sandboxed iframe standing in for the text
     diff. Themed background shows while its isolated document loads (and behind a
     transparent HTML body); the document itself carries its own light/dark CSS. */
  .render {
    display: block;
    block-size: 360px;
    inline-size: 100%;
    border: 0;
    background: var(--surface-1);
  }

  /* Image preview: the rendered picture in place of a text diff. A subtle
     checkerboard (a token-tinted `color-mix`, so it adapts to light/dark on its
     own) sits under it, letting transparent PNG/SVG regions read in both themes. */
  .imgwrap {
    --checker: color-mix(in sRGB, var(--on-surface) 7%, transparent);

    display: grid;
    place-items: center;
    overflow: auto;
    max-block-size: 300px;
    padding: 12px;
    background-color: var(--code-background);
    background-image:
      linear-gradient(45deg, var(--checker) 25%, transparent 25%),
      linear-gradient(-45deg, var(--checker) 25%, transparent 25%),
      linear-gradient(45deg, transparent 75%, var(--checker) 75%),
      linear-gradient(-45deg, transparent 75%, var(--checker) 75%);
    background-position:
      0 0,
      0 8px,
      8px -8px,
      -8px 0;
    background-size: 16px 16px;

    img {
      object-fit: contain;
      block-size: auto;
      max-block-size: 276px;
      max-inline-size: 100%;
      border-radius: var(--radius-small);

      /* A hairline keeps a light image legible against the light checkerboard. */
      box-shadow: 0 0 0 1px color-mix(in sRGB, var(--outline) 60%, transparent);
    }
  }
</style>
