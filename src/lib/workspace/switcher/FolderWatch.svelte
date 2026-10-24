<script lang="ts">
  import { Command, EventType, onMessage, sendMessage } from "@/lib/ipc/messaging";
  import { watchedDirectories } from "@/panels/picker/project-roots";
  import type { UnlistenFn } from "@tauri-apps/api/event";
  import { onDestroy, onMount, untrack } from "svelte";

  // The left rail's eyes on the folders it lists, so one deleted or renamed
  // outside PADE leaves the list while it is on screen. It watches the PARENTS
  // of the listed projects, never the projects - a watch holds a handle on its
  // folder, and a handle on a project is the very thing that would stop it
  // being deleted - and only while `isActive`, so a closed menu holds nothing.
  const { projects, isActive, onvanish }: {
    projects: string[];
    isActive: boolean;
    /** A listed folder may have gone - forget the vanished ones. */
    onvanish: () => Promise<void>;
  } = $props();

  /** A delete lands as several events; act once it has settled. */
  const SETTLE_MILLISECONDS = 250;

  let settleTimer: ReturnType<typeof setTimeout> | undefined;
  let unlistenDirectories: UnlistenFn | undefined;

  async function watchFolders(dirs: string[]) {
    try {
      await sendMessage(Command.watchDirs, { dirs });
    } catch (error) {
      console.warn("pade: could not watch the switcher's folders", error);
    }
  }

  $effect(() => {
    const dirs = isActive
      ? watchedDirectories({
        roots: [],
        recentProjects: projects
      })
      : [];
    watchFolders(dirs);
  });

  // A folder that went while the menu was closed is caught as it opens.
  $effect(() => {
    if (isActive) {
      untrack(onvanish);
    }
  });

  onMount(async () => {
    unlistenDirectories = await onMessage(EventType.directoriesChanged, () => {
      clearTimeout(settleTimer);
      settleTimer = setTimeout(onvanish, SETTLE_MILLISECONDS);
    });
  });

  onDestroy(() => {
    clearTimeout(settleTimer);
    unlistenDirectories?.();
    watchFolders([]);
  });
</script>
