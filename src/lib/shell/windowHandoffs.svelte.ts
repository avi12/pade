// Sending sessions to other PADE windows and taking in what they send here.
// One mechanism serves every case (lib/shell/window-handoff for the payload,
// `window_handoff.rs` for the PTY move): a live session shift-clicked in the
// rail moves into a new window, a whole window merges into another before it
// closes, and a conversation or chat shift-clicked reopens in a new window. A
// moved session keeps its process - only its owner changes - and the
// receiving window re-attaches its terminal.

import { Command, EventType, onMessage, sendMessage } from "@/lib/ipc/messaging";
import { closeWindow } from "@/lib/ipc/platform";
import { liveSessionOf, snapshotSessionOf } from "@/lib/session/session-restore";
import {
  HandoffKind,
  handoffPayload,
  handoffProject,
  joinTargetOf,
  parseHandoffs
} from "@/lib/shell/window-handoff";
import type { SessionsHandoff, WindowHandoff } from "@/lib/shell/window-handoff";
import type { WindowSessions } from "@/lib/shell/windowSessions.svelte";
import { sessionLabel, setSessionLabel } from "@/lib/stores/session/sessionLabels.svelte";
import { showToast } from "@/lib/stores/ui/toast.svelte";
import { WindowMode } from "@/lib/types";
import type { AgentSession, OpenTarget } from "@/lib/types";
import type { UnlistenFn } from "@tauri-apps/api/event";
import { onDestroy, onMount } from "svelte";

/** What the shell lends the hand-offs. */
interface WindowHandoffsHost {
  windowSessions: WindowSessions;
  /** Enter a project beside the sessions already here (a conversation or chat
   *  handed to this window). */
  joinProject: (target: OpenTarget) => Promise<void>;
  /** Take in a project the handed sessions belong to - the backend records it
   *  as recently opened. */
  openWorkspace: (path: string) => Promise<void>;
  /** This window just gave away its last session. */
  onEmptiedByMove: () => void;
}

export function createWindowHandoffs(host: WindowHandoffsHost) {
  const { windowSessions } = host;

  /** The hand-off that carries these live sessions, labels and all. */
  function sessionsHandoff(sessions: readonly AgentSession[]): WindowHandoff {
    return {
      kind: HandoffKind.enum.sessions,
      sessions: sessions.map(session => ({
        ...snapshotSessionOf(session),
        label: sessionLabel(session.id)
      })),
      activeId: windowSessions.activeId
    };
  }

  /** Drop what was handed away here, and leave the window to its picker when
   *  nothing is left. */
  function release(ids: readonly string[]) {
    windowSessions.release(ids);

    if (windowSessions.sessions.length === 0) {
      host.onEmptiedByMove();
    }
  }

  /** Open a new window that receives `handoff` (and the PTYs of `sessions`). */
  async function sendToNewWindow({ handoff, sessions }: {
    handoff: WindowHandoff;
    sessions: readonly AgentSession[];
  }) {
    try {
      await sendMessage(Command.windowCreate, {
        mode: WindowMode.enum.handoff,
        path: handoffProject(handoff),
        sessions: sessions.map(session => session.id),
        payload: handoffPayload(handoff)
      });
    } catch (error) {
      console.warn("pade: could not open a window for the hand-off", error);
      showToast("Couldn't open a new window for that");
      return false;
    }

    return true;
  }

  /** Move one live session - still running - into a new window of its own. */
  async function moveToNewWindow(session: AgentSession) {
    const isSent = await sendToNewWindow({
      handoff: sessionsHandoff([session]),
      sessions: [session]
    });
    if (isSent) {
      release([session.id]);
    }
  }

  /** Reopen a conversation or a persisted chat in a new window on its project. */
  async function reopenInNewWindow(handoff: WindowHandoff) {
    await sendToNewWindow({
      handoff,
      sessions: []
    });
  }

  /** Move every session of this window into the open window `label`, then
   *  close this one - its sessions keep running there. */
  async function mergeInto(label: string) {
    const sessions = [...windowSessions.sessions];
    if (sessions.length > 0) {
      const isMoved = await sendMessage(Command.windowHandOff, {
        to: label,
        sessions: sessions.map(session => session.id),
        payload: handoffPayload(sessionsHandoff(sessions))
      });
      if (!isMoved) {
        showToast("That window is gone");
        return;
      }

      windowSessions.release(sessions.map(session => session.id));
    }

    await closeWindow();
  }

  /** Take in one hand-off another window sent here. */
  async function receive(handoff: WindowHandoff) {
    await host.openWorkspace(handoffProject(handoff));

    if (handoff.kind === HandoffKind.enum.sessions) {
      adoptSessions(handoff);
      return;
    }

    await host.joinProject(joinTargetOf(handoff));
  }

  /** Take in live sessions, their PTYs already moved here, titled as they were. */
  function adoptSessions(handoff: SessionsHandoff) {
    for (const handed of handoff.sessions) {
      if (handed.label !== undefined) {
        setSessionLabel({
          id: handed.id,
          label: handed.label
        });
      }
    }

    windowSessions.adopt({
      adopted: handoff.sessions.map(saved => liveSessionOf({
        saved,
        fallbackProject: saved.project
      })),
      activeId: handoff.activeId
    });
  }

  /** Take every hand-off waiting for this window. True when there was any. */
  async function takeHandoffs() {
    const handoffs = parseHandoffs(await sendMessage(Command.windowTakeHandoffs));
    for (const handoff of handoffs) {
      await receive(handoff);
    }

    return handoffs.length > 0;
  }

  // A running window is told when another hands it sessions (a merge).
  let unlistenHandoff: UnlistenFn | undefined;
  onMount(async () => {
    unlistenHandoff = await onMessage(EventType.windowHandoff, async () => {
      await takeHandoffs();
    });
  });
  onDestroy(() => unlistenHandoff?.());

  return {
    moveToNewWindow,
    reopenInNewWindow,
    mergeInto,
    takeHandoffs
  };
}
