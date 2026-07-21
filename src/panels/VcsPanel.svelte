<script lang="ts">
  import { feed, vcs } from "@/lib/bridge";
  import { setPanelHeader } from "@/lib/stores/sidePanel.svelte";
  import type { Commit, StatusEntry } from "@/lib/types";
  import ChangesSection from "@/panels/vcs/ChangesSection.svelte";
  import "@/panels/vcs/chrome.css";
  import CommitLog from "@/panels/vcs/CommitLog.svelte";
  import RestoreSection from "@/panels/vcs/RestoreSection.svelte";
  import type { UnlistenFn } from "@tauri-apps/api/event";
  import { onDestroy, onMount } from "svelte";

  const { project }: { project: string } = $props();

  let entries = $state<StatusEntry[]>([]);
  let commits = $state<Commit[]>([]);
  let error = $state<string | null>(null);
  let unlisten: UnlistenFn | undefined;
  let unlistenGitState: UnlistenFn | undefined;
  let refreshVersion = 0;

  async function refresh() {
    const workspace = project;
    const version = ++refreshVersion;
    if (!workspace) {
      entries = [];
      commits = [];
      error = null;
      return;
    }

    try {
      const [nextEntries, nextCommits] = await Promise.all([
        vcs.status(workspace),
        vcs.log(workspace)
      ]);
      // A late response for the prior project must not repaint this panel after
      // the window has switched to another workspace.
      if (version !== refreshVersion || workspace !== project) {
        return;
      }

      entries = nextEntries;
      commits = nextCommits;
      error = null;
    } catch (e) {
      if (version !== refreshVersion || workspace !== project) {
        return;
      }

      error = String(e);
      entries = [];
      commits = [];
    }
  }

  // Debounced refresh so a burst of saves triggers one status fetch.
  let timer: ReturnType<typeof setTimeout> | undefined;
  function scheduleRefresh() {
    clearTimeout(timer);
    timer = setTimeout(refresh, 400);
  }

  onMount(async () => {
    unlisten = await feed.onChange(scheduleRefresh);
    // A branch switch, remote change, or git init reshapes the status and the
    // log without necessarily touching a watched file — refresh on it too.
    unlistenGitState = await vcs.onStateChanged(scheduleRefresh);
  });
  onDestroy(() => {
    unlisten?.();
    unlistenGitState?.();
    clearTimeout(timer);
  });

  // The lazy panel remains alive when this window switches projects, so route
  // its fresh query through the new explicit workspace.
  $effect(() => {
    void refresh();
  });

  // Publish the refresh action to the shared side-panel header.
  $effect(() => {
    setPanelHeader({
      count: null,
      refresh
    });
  });
</script>

<div class="vcs">
  {#if error}
    <p class="empty">Not a Git repository, or git is unavailable.</p>
  {:else}
    <div class="scroll">
      <RestoreSection {project} />

      <ChangesSection {entries} {project} />

      <CommitLog {commits} {project} />
    </div>
  {/if}
</div>

<style>
  .vcs {
    display: flex;
    flex-direction: column;
    height: 100%;
  }

  .scroll {
    display: flex;
    flex: 1;
    flex-direction: column;
    gap: 14px;
    overflow-y: auto;
    min-block-size: 0;
    padding: 8px 10px;
  }

  /* Shared panel chrome (group headers, sha, author line, empty state) lives
     in vcs/chrome.css so the section components share one copy. */

  /* The sections own their styles: vcs/RestoreSection.svelte,
     vcs/ChangesSection.svelte, vcs/CommitLog.svelte; shared chrome is
     vcs/chrome.css. */
</style>
