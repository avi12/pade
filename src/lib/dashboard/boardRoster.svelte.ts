// A project window's view of the sessions every OTHER PADE window runs, for
// its left rail: live rows marked as running elsewhere, which a click brings
// forward in their own window. It listens on the agents board (no new channel)
// and asks the windows to publish with a roll call, which - unlike the
// dashboard's announcement - costs them no summaries.

import { BoardMessageKind, elsewhereSessions } from "@/lib/dashboard/agent-board";
import { createBoardWindows } from "@/lib/dashboard/boardWindows.svelte";
import { windowLabel } from "@/lib/ipc/platform";

export function createBoardRoster() {
  const board = createBoardWindows({
    announcement: {
      kind: BoardMessageKind.enum.rollCall
    }
  });
  const ownLabel = windowLabel();
  const sessions = $derived(
    elsewhereSessions({
      windows: board.windows,
      ownLabel
    })
  );

  return {
    /** The sessions the other windows run right now. */
    get sessions() {
      return sessions;
    },
    focus: board.focus
  };
}
