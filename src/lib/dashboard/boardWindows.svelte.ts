// The listening side of the agents board (lib/dashboard/agent-board), shared by
// the dashboard window and every project window's left rail: keep each other
// window's latest snapshot, drop a window once it leaves - by its own goodbye,
// or by no longer being among the open windows when one closed without saying
// so - and ask a window to bring one of its sessions forward.

import {
  BOARD_CHANNEL,
  NO_BOARD_WINDOWS,
  parseBoardMessage,
  windowsAfterMessage,
  windowsStillOpen
} from "@/lib/dashboard/agent-board";
import type { BoardMessage, BoardWindow } from "@/lib/dashboard/agent-board";
import { BoardMessageKind } from "@/lib/dashboard/agent-board";
import { Command, EventType, onMessage, sendMessage } from "@/lib/ipc/messaging";
import type { UnlistenFn } from "@tauri-apps/api/event";
import { onDestroy, onMount } from "svelte";

export function createBoardWindows({ announcement }: {
  /** What this listener posts as it starts, so the windows publish to it. */
  announcement: BoardMessage;
}) {
  const channel = new BroadcastChannel(BOARD_CHANNEL);
  // Replaced whole on every change (the pure rules return a new map), so a raw
  // state of an immutable map is all the reactivity it needs.
  let windows = $state.raw<ReadonlyMap<string, BoardWindow>>(NO_BOARD_WINDOWS);
  /** The open windows' labels in the switcher's order (`window_list`). */
  let order = $state<string[]>([]);

  function post(message: BoardMessage) {
    channel.postMessage(message);
  }

  /** Follow the open windows: their order, and which have closed. */
  async function syncOpenWindows() {
    try {
      order = (await sendMessage(Command.windowList)).map(listed => listed.label);
    } catch (error) {
      console.warn("pade: could not list the open windows, keeping the board as it is", error);
      return;
    }

    windows = windowsStillOpen({
      windows,
      openLabels: order
    });
  }

  onMount(() => {
    channel.onmessage = e => {
      const parsed = parseBoardMessage(e.data);
      if (parsed.success) {
        windows = windowsAfterMessage({
          windows,
          message: parsed.data
        });
      }
    };
    post(announcement);
  });

  let unlistenWindows: UnlistenFn | undefined;
  onMount(async () => {
    await syncOpenWindows();
    unlistenWindows = await onMessage(EventType.windowsChanged, syncOpenWindows);
  });
  onDestroy(() => {
    unlistenWindows?.();
    channel.close();
  });

  return {
    get windows() {
      return windows;
    },
    /** The open windows' labels in the user's "Open windows" order. */
    get order() {
      return order;
    },
    post,
    /** Bring a window forward on the session the user picked. */
    async focus({ label, sessionId }: {
      label: string;
      sessionId?: string;
    }) {
      if (sessionId !== undefined) {
        post({
          kind: BoardMessageKind.enum.focus,
          label,
          sessionId
        });
      }

      await sendMessage(Command.windowFocusLabel, { label });
    }
  };
}
