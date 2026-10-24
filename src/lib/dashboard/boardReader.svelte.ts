// The dashboard window's side of the agents board (lib/dashboard/agent-board):
// announce itself so every project window publishes (and starts summarizing),
// and keep announcing while open. Following the windows' snapshots is the
// shared listener's job (lib/dashboard/boardWindows).

import { BoardMessageKind, DASHBOARD_HEARTBEAT_MS } from "@/lib/dashboard/agent-board";
import { createBoardWindows } from "@/lib/dashboard/boardWindows.svelte";
import { onMount } from "svelte";

export function createBoardReader() {
  const board = createBoardWindows({
    announcement: {
      kind: BoardMessageKind.enum.dashboardOpen
    }
  });

  onMount(() => {
    const heartbeat = setInterval(
      () => board.post({ kind: BoardMessageKind.enum.dashboardOpen }),
      DASHBOARD_HEARTBEAT_MS
    );
    function leave() {
      board.post({ kind: BoardMessageKind.enum.dashboardClosed });
    }

    addEventListener("pagehide", leave);
    return () => {
      clearInterval(heartbeat);
      removeEventListener("pagehide", leave);
    };
  });

  return board;
}
