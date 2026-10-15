<script lang="ts">
  import { groupChanges } from "@/lib/feed/change-groups";
  import { createFeedPreviews } from "@/lib/feed/feed-previews";
  import { watchSummary } from "@/lib/feed/feed-status";
  import { countExtensions, filterByExtension } from "@/lib/feed/type-filter";
  import { Command, EventType, onMessage, sendMessage } from "@/lib/ipc/messaging";
  import { effective } from "@/lib/settings/prefs.svelte";
  import { setPanelHeader } from "@/lib/stores/ui/sidePanel.svelte";
  import { showToast } from "@/lib/stores/ui/toast.svelte";
  import { ensureEditors } from "@/lib/stores/workspace/editors.svelte";
  import { feedStore, retargetFeed } from "@/lib/stores/workspace/feed.svelte";
  import { workspaceMembers as storedMembers } from "@/lib/stores/workspace/workspaceMembers.svelte";
  import type { ChangeEvent, WatchStatus } from "@/lib/types";
  import FeedCards from "@/panels/feed/FeedCards.svelte";
  import FeedEmptyState from "@/panels/feed/FeedEmptyState.svelte";
  import FeedToolbar from "@/panels/feed/FeedToolbar.svelte";
  import GroupChips from "@/panels/feed/GroupChips.svelte";
  import type { UnlistenFn } from "@tauri-apps/api/event";
  import { onDestroy, onMount } from "svelte";
  import { SvelteSet } from "svelte/reactivity";

  // The open project's root dir - lets an editor open a change in the window
  // that already has this project, at the right line.
  const { project }: { project: string } = $props();

  // The feed events live in a persistent store (lib/stores/workspace/feed) that owns the one
  // live subscription, so switching side panels away and back keeps the feed -
  // this component only reads and renders them.

  // "Sync all" only makes sense when there's a remote to fast-forward from - a
  // fresh local project (git-init'd but no origin) or a non-repo has nothing to
  // pull. `remoteUrl` is null with no remote and rejects outside a repo, so the
  // button stays hidden in both cases and appears only for a repo with a remote.
  async function loadRemote(root: string) {
    try {
      const remote = await sendMessage(Command.vcsRemoteUrl, { cwd: root });
      if (root !== project) {
        return;
      }

      hasRemote = remote !== null;
    } catch {
      if (root !== project) {
        return;
      }

      hasRemote = false;
    }
  }

  // The workspace's members (backend census: a package by its manifest, a
  // checkout by its own `.git`) - the grouping ground truth change-groups
  // prefers over folder-name conventions. The shell's windowServices discovers them once per project
  // into the shared store; with none, the convention fallback stays in charge.
  const workspaceMembers = $derived(storedMembers());

  // Whether the workspace has a git remote to sync from - gates the whole button
  // (see loadRemote). A fresh local project or a non-repo has nothing to pull.
  let hasRemote = $state(false);

  // Inline viewer: only one card expands at a time.
  let expandedId = $state<string | null>(null);
  // Seed the viewer's Unified|Split layout from the saved preference.
  let diffMode = $state(effective.diffStyle);
  // Each card's lazily fetched preview, cached per event id for this mount.
  const previews = createFeedPreviews();

  // A reactive clock so relative timestamps ("3m ago") tick forward on their own.
  let now = $state(Date.now());
  let clock: ReturnType<typeof setInterval> | undefined;
  // The live git-state subscription - a branch switch, remote change, or git
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
    unlistenGitState = await onMessage(EventType.gitState, () => {
      loadRemote(project);
    });
  });

  onDestroy(() => {
    if (clock !== undefined) {
      clearInterval(clock);
    }

    unlistenGitState?.();
  });

  // The file-watcher itself is started and re-rooted by the shell's windowServices (keyed on
  // the open project), so the feed just subscribes to its stream above - no need
  // to arm it here.

  // The workspace's remote gating, watch status and editors, loaded together
  // (concurrently) on mount and on a project switch. Each callee owns its own
  // error handling, so the gathered promise never rejects. The members and their
  // branches are not here: they belong to the shared store windowServices fills.
  async function loadWorkspaceState(workspace: string) {
    await Promise.all([
      loadRemote(workspace),
      // What the backend is actually watching, so an empty feed can say why.
      loadWatchStatus(),
      // Editors come from the shared store's cache on a mere remount (a side-panel
      // switch); a fetch only runs when nothing is cached for this project yet.
      ensureEditors(workspace)
    ]);
  }

  // Re-read the workspace branch on mount and whenever the window switches
  // projects, and point the persistent feed at the current project so a workspace
  // switch clears stale events even if the panel was closed during the switch.
  // Reacts to `project` - the loads write state this effect doesn't read, and the
  // editors store's trigger functions are reactivity-transparent (its bookkeeping
  // maps are non-reactive on purpose), so this fires per workspace, never per tick.
  $effect(() => {
    const workspace = project;
    if (!workspace) {
      return;
    }

    retargetFeed(workspace);
    loadWorkspaceState(workspace);
  });

  const expandedEvent = $derived(feedStore.events.find(item => item.id === expandedId) ?? null);

  // Move the open card to a newer revision of its file only once that
  // revision's preview is cached, so it never flashes "No preview available".
  // The user may have closed or switched cards meanwhile - then stay put.
  async function followNewestRevision({ staleId, newest }: {
    staleId: string;
    newest: ChangeEvent;
  }) {
    await previews.loadCardPreview(newest);
    const isStillOnStaleCard = expandedId === staleId;
    if (isStillOnStaleCard) {
      expandedId = newest.id;
    }
  }

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
      followNewestRevision({
        staleId: open.id,
        newest: newestForPath
      });
    }
  });

  // ── Grouping + filters ──────────────────────────────────────────────────────
  // Bucket the feed by project (manifest members first, folder-name convention
  // as the fallback - see change-groups), and let the chip row narrow to one
  // project and the "File type" filter narrow to a set of file extensions.
  let activeGroupId = $state<string | null>(null);

  // The per-extension tally over the live feed - the one source both the filter
  // popover's counts and its filtering read (SSOT).
  const extensionCounts = $derived(countExtensions(feedStore.events));

  // Opt-in file-type filter: the extensions the user has ticked. An empty set is
  // the default and means "no filter - show every type", so a checkbox is empty
  // by default and the feed stays fully visible until a type is picked.
  const selectedExtensions = new SvelteSet<string>();

  const groups = $derived(
    groupChanges({
      events: filterByExtension({
        events: feedStore.events,
        selected: selectedExtensions
      }),
      workspaceRoot: project,
      members: workspaceMembers
    })
  );
  // A chip selection can outlive its group (its events filtered out or aged past
  // the cap); when it does, fall back to showing every group.
  const selectionStillExists = $derived(
    activeGroupId !== null && groups.some(group => group.id === activeGroupId)
  );
  // The chip row only offers a choice when there is one to make - or to undo.
  const isChipRowShown = $derived(groups.length > 1 || activeGroupId !== null);
  const visibleGroups = $derived.by(() => {
    if (activeGroupId === null || !selectionStillExists) {
      return groups;
    }

    return groups.filter(group => group.id === activeGroupId);
  });

  // Publish the live event count to the shared side-panel header.
  $effect(() => {
    setPanelHeader({
      count: feedStore.events.length,
      refresh: null
    });
  });

  // ── The empty panel's self-description ──────────────────────────────────────
  // An empty list is ambiguous: nothing changed, the watch is on another
  // project, or there is no watch. The backend knows which (watch_status), so
  // the panel states it instead of always saying "waiting for edits". Read on
  // mount / project switch, and again whenever the window is looked at again -
  // the moment the answer could have gone stale unnoticed.
  let watchStatus = $state<WatchStatus | null>(null);

  // Never rejects: a diagnostic that fails must not take the panel's other loads
  // (branch, members, editors) down with it, so the failure is surfaced as a
  // toast and the summary falls back to its not-known-yet wording.
  async function loadWatchStatus() {
    try {
      watchStatus = await sendMessage(Command.watchStatus);
    } catch (error) {
      watchStatus = null;
      showToast(`Change Feed status unavailable - ${String(error)}`);
    }
  }
</script>

<svelte:document
  onvisibilitychange={async () => {
    if (!document.hidden && feedStore.events.length === 0) {
      await loadWatchStatus();
    }
  }}
/>

<div class="feed">
  {#if feedStore.events.length === 0}
    <FeedEmptyState
      onArmed={loadWatchStatus}
      {project}
      summary={watchSummary({
        status: watchStatus,
        project,
        now
      })}
    />
  {:else}
    <FeedToolbar {extensionCounts} {hasRemote} {project} {selectedExtensions} />

    {#if isChipRowShown}
      <GroupChips {groups} totalCount={feedStore.events.length} bind:activeGroupId />
    {/if}

    <FeedCards groups={visibleGroups} {now} {previews} {project} bind:diffMode bind:expandedId />
  {/if}
</div>

<style>
  .feed {
    display: flex;
    flex-direction: column;
    height: 100%;
  }
</style>
