// Re-attach after the window loses its webview - the session-persistence seam.
//
// Two things take the UI away while the agents keep running: an accidental
// reload (F5, a dropped HMR socket) and a dead WebView2 browser process, which
// `recovery.rs` answers by destroying the window and rebuilding it under the
// same label and URL. The rebuild is why the pane-mapping snapshot is stored
// per window label in localStorage and not in sessionStorage: sessionStorage
// dies with the webview, so a rebuilt window booted with no record of what it
// owned, spawned a fresh agent, and left the live PTYs orphaned in the backend
// - running, billing, reachable by nothing.
//
// Outliving the webview is safe because liveness is never this module's claim
// to make. The backend's PtyState stays the sole authority: restore only
// re-attaches sessions `pty_list` still hosts, and it lists only the ones this
// window owns. A deliberate leave kills its PTYs first and app exit kills them
// all - so nothing survives the intersection, a snapshot left behind by an
// earlier run finds nothing live and is cleared, and no separate "leave intent"
// flag is needed.

import { Command, sendMessage } from "@/lib/ipc/messaging";
import { windowLabel } from "@/lib/ipc/platform";
import { Agent, ModelChoice } from "@/lib/types";
import type { AgentSession } from "@/lib/types";
import { readStoredJson, writeStoredJson } from "@/lib/util/stored-json";
import { z } from "zod";

/** Where this window's snapshot lives. Keyed by window label so a sibling
 *  window never reads (or overwrites) another's pane mapping, and so the
 *  crash-rebuilt window - same label - finds its own. */
function snapshotStorageKey() {
  return `pade.session-snapshot:${windowLabel()}`;
}

/** One persisted session - `AgentSession` minus `initialPrompt`, which was
 *  already submitted into the live conversation (restoring it would make the
 *  re-attached terminal send it again). */
export const SnapshotSession = z.object({
  id: z.string(),
  agent: Agent,
  // The project the session belongs to - a window holds several. A snapshot
  // written before sessions carried one falls back to the snapshot's project.
  project: z.string().min(1).optional(),
  cwd: z.string().optional(),
  branch: z.string().optional(),
  args: z.array(z.string()).optional(),
  // Kept so a session that re-attached after a reload can still be restarted
  // back into its own conversation when its MCP config later changes.
  conversationId: z.string().optional(),
  // A model picked for this session alone survives a reload, so its next
  // relaunch keeps running it.
  modelChoice: ModelChoice.optional()
});

/** The persisted shape of one session, as a snapshot or a hand-off carries it. */
export function snapshotSessionOf({
  id,
  agent,
  project,
  cwd,
  branch,
  args,
  conversationId,
  modelChoice
}: AgentSession) {
  return {
    id,
    agent,
    project,
    cwd,
    branch,
    args,
    conversationId,
    modelChoice
  } satisfies z.infer<typeof SnapshotSession>;
}

/** A persisted session back as a live one, in `fallbackProject` when it was
 *  saved without its own. */
export function liveSessionOf({ saved, fallbackProject }: {
  saved: z.infer<typeof SnapshotSession>;
  fallbackProject: string;
}): AgentSession {
  return {
    ...saved,
    project: saved.project ?? fallbackProject
  };
}

/** What a window needs to re-attach after a reload: the project in front and
 *  the session/pane layout it was showing. */
const SessionSnapshot = z.object({
  project: z.string().min(1),
  sessions: z.array(SnapshotSession).min(1),
  paneIds: z.array(z.string()),
  activeId: z.string().nullable()
});
export type SessionSnapshot = z.infer<typeof SessionSnapshot>;

/** Persist the window's current pane mapping. An empty project or session list
 *  means there is nothing to re-attach, so the snapshot is cleared instead. */
export function saveSessionSnapshot({ project, sessions, paneIds, activeId }: {
  project: string;
  sessions: readonly AgentSession[];
  paneIds: readonly string[];
  activeId: string | null;
}) {
  const isNothingToReattach = project === "" || sessions.length === 0;
  if (isNothingToReattach) {
    clearSessionSnapshot();
    return;
  }

  const snapshot: SessionSnapshot = {
    project,
    sessions: sessions.map(snapshotSessionOf),
    paneIds: [...paneIds],
    activeId
  };
  writeStoredJson({
    key: snapshotStorageKey(),
    value: snapshot
  });
}

export function clearSessionSnapshot() {
  localStorage.removeItem(snapshotStorageKey());
}

/** The persisted snapshot, or `null` when absent or malformed. */
export function readSessionSnapshot() {
  return readStoredJson({
    key: snapshotStorageKey(),
    schema: SessionSnapshot
  });
}

/** The snapshot cut down to the sessions the backend still hosts - pure, so the
 *  intersection is unit-testable. Panes and the active id are pruned with it
 *  (a survivor always ends up shown); `null` when no session survived, which is
 *  exactly what a deliberate leave (PTYs killed) looks like. */
export function pruneToLive({ snapshot, liveIds }: {
  snapshot: SessionSnapshot;
  liveIds: ReadonlySet<string>;
}) {
  const sessions = snapshot.sessions
    .filter(session => liveIds.has(session.id))
    .map(saved => liveSessionOf({
      saved,
      fallbackProject: snapshot.project
    }));
  if (sessions.length === 0) {
    return null;
  }

  const survivingIds = new Set(sessions.map(session => session.id));
  const shownPaneIds = snapshot.paneIds.filter(id => survivingIds.has(id));
  const paneIds = shownPaneIds.length > 0 ? shownPaneIds : [sessions[0].id];

  const hasActiveSurvived = snapshot.activeId !== null && survivingIds.has(snapshot.activeId);
  const activeId = hasActiveSurvived ? snapshot.activeId : (paneIds.at(-1) ?? null);

  return {
    project: snapshot.project,
    sessions,
    paneIds,
    activeId
  };
}

/** The snapshot a reloaded window can actually re-attach to: the persisted pane
 *  mapping intersected with the backend's live-session roster (`pty_list`). A
 *  snapshot with nothing live behind it is stale - cleared and `null`. */
export async function restoreLiveSnapshot() {
  const snapshot = readSessionSnapshot();
  if (!snapshot) {
    return null;
  }

  const live = await sendMessage(Command.ptyList);
  const restorable = pruneToLive({
    snapshot,
    liveIds: new Set(live.map(session => session.id))
  });
  if (!restorable) {
    clearSessionSnapshot();
  }

  return restorable;
}
