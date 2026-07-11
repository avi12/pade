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

  let entries = $state<StatusEntry[]>([]);
  let commits = $state<Commit[]>([]);
  let error = $state<string | null>(null);
  let unlisten: UnlistenFn | undefined;

  async function refresh() {
    try {
      [entries, commits] = await Promise.all([vcs.status(), vcs.log()]);
      error = null;
    } catch (e) {
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
    await refresh();
    unlisten = await feed.onChange(scheduleRefresh);
  });
  onDestroy(() => {
    unlisten?.();
    clearTimeout(timer);
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
      <RestoreSection />

      <ChangesSection {entries} />

      <CommitLog {commits} />
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
