import { onMount } from "svelte";

import { onWindowCloseRequested } from "@/lib/ipc/platform";
import { clearDiscordPresence } from "@/lib/shell/window/discord-presence";
import type { WindowSessions } from "@/lib/shell/windowSessions.svelte";
import { stopAllRunners } from "@/lib/stores/tasks/runners.svelte";

/** Deliberate leaves of the window's workspace: switching project, going back
 *  to the picker, closing PADE. */
export function createWindowLeave({
  windowSessions,
  isWindowFocused
}: {
  windowSessions: WindowSessions;
  isWindowFocused: () => boolean;
}) {
  // One deliberate leave at a time. The graceful wait can hold the UI open for
  // a while it stays fully interactive, so a second switch/leave starting
  // inside that window would interleave two kill → open → launch sequences over
  // the same session list. Later attempts are dropped, not queued - the first
  // leave is already taking the user away.
  let isLeaveInFlight = false;
  async function runExclusiveLeave(leave: () => Promise<void>) {
    if (isLeaveInFlight) {
      return;
    }

    isLeaveInFlight = true;
    try {
      await leave();
    } finally {
      isLeaveInFlight = false;
    }
  }

  // Stop every runner and close every session on a deliberate workspace leave.
  // The runner store is scoped to this window, so clearing it also removes the
  // old workspace's dock before the new project paints.
  async function closeWorkspaceGracefully() {
    await stopAllRunners();
    await windowSessions.closeAllGracefully();
  }

  // Closing PADE (the title-bar X) is a deliberate leave too: intercept the
  // close, let every agent - of every project this window holds - reach an
  // idle prompt, kill them, and only then let the window go (Tauri destroys it
  // once the handler settles unprevented). While a leave is already in flight
  // the gate skips the wait, so a second X-click closes immediately -
  // graceful, never a trap; the backend ends whatever PTYs the window still
  // owns as it is destroyed (window.rs `on_destroyed`).
  let unlistenCloseRequested: (() => void) | undefined;
  async function interceptWindowClose() {
    unlistenCloseRequested = await onWindowCloseRequested(async () => {
      // This window is going away: if it's the one publishing presence (only the
      // focused window does), stop reporting so its last project doesn't linger.
      // A remaining PADE window re-publishes its own status on gaining focus.
      if (isWindowFocused()) {
        await clearDiscordPresence();
      }

      await runExclusiveLeave(closeWorkspaceGracefully);
    });
  }
  onMount(() => {
    void interceptWindowClose();
    return () => unlistenCloseRequested?.();
  });

  return {
    get isInFlight() {
      return isLeaveInFlight;
    },
    runExclusiveLeave,
    closeWorkspaceGracefully
  };
}

export type WindowLeave = ReturnType<typeof createWindowLeave>;
