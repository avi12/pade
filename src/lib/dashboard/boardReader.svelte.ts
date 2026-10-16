// The dashboard window's side of the agents board (lib/dashboard/agent-board):
// announce itself so every project window publishes (and starts summarizing),
// keep each window's latest snapshot, and drop a window once it leaves - by its
// own goodbye, or by no longer being among the open windows when one closed
// without saying so.

import {
  BOARD_CHANNEL,
  BoardMessageKind,
  DASHBOARD_HEARTBEAT_MS,
  parseBoardMessage
} from "@/lib/dashboard/agent-board";
import type { BoardMessage, BoardWindow } from "@/lib/dashboard/agent-board";
import { Command, EventType, onMessage, sendMessage } from "@/lib/ipc/messaging";
import type { UnlistenFn } from "@tauri-apps/api/event";
import { onDestroy, onMount } from "svelte";
import { SvelteMap } from "svelte/reactivity";

export function createBoardReader() {
  const channel = new BroadcastChannel(BOARD_CHANNEL);
  const windows = new SvelteMap<string, BoardWindow>();

  function post(message: BoardMessage) {
    channel.postMessage(message);
  }

  async function dropClosedWindows() {
    const open = (await sendMessage(Command.windowList)).map(listed => listed.label);
    for (const label of windows.keys()) {
      if (!open.includes(label)) {
        windows.delete(label);
      }
    }
  }

  onMount(() => {
    channel.onmessage = e => {
      const parsed = parseBoardMessage(e.data);
      if (!parsed.success) {
        return;
      }

      const message = parsed.data;
      if (message.kind === BoardMessageKind.enum.window) {
        windows.set(message.window.label, message.window);
      } else if (message.kind === BoardMessageKind.enum.windowGone) {
        windows.delete(message.label);
      }
    };
    post({ kind: BoardMessageKind.enum.dashboardOpen });
    const heartbeat = setInterval(() => post({ kind: BoardMessageKind.enum.dashboardOpen }), DASHBOARD_HEARTBEAT_MS);
    const leave = () => post({ kind: BoardMessageKind.enum.dashboardClosed });
    addEventListener("pagehide", leave);
    return () => {
      clearInterval(heartbeat);
      removeEventListener("pagehide", leave);
    };
  });

  let unlistenWindows: UnlistenFn | undefined;
  onMount(async () => {
    unlistenWindows = await onMessage(EventType.windowsChanged, async () => {
      try {
        await dropClosedWindows();
      } catch (error) {
        console.warn("pade: could not list the open windows, keeping the board as it is", error);
      }
    });
  });
  onDestroy(() => {
    unlistenWindows?.();
    channel.close();
  });

  return {
    get windows() {
      return windows;
    },
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
