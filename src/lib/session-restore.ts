// Re-attach after the window loses its webview — the session-persistence seam.
//
// Two things take the UI away while the agents keep running: an accidental
// reload (F5, a dropped HMR socket) and a dead WebView2 browser process, which
// `recovery.rs` answers by destroying the window and rebuilding it under the
// same label and URL. The rebuild is why the pane-mapping snapshot is stored
// per window label in localStorage and not in sessionStorage: sessionStorage
// dies with the webview, so a rebuilt window booted with no record of what it
// owned, spawned a fresh agent, and left the live PTYs orphaned in the backend
// — running, billing, reachable by nothing.
//
// Outliving the webview is safe because liveness is never this module's claim
// to make. The backend's PtyState stays the sole authority: restore only
// re-attaches sessions `pty_list` still hosts, and it lists only the ones this
// window owns. A deliberate leave kills its PTYs first and app exit kills them
// all — so nothing survives the intersection, a snapshot left behind by an
// earlier run finds nothing live and is cleared, and no separate "leave intent"
// flag is needed.

import { pty, windows } from "@/lib/bridge";
import { Agent } from "@/lib/types";
import type { AgentSession } from "@/lib/types";
import { z } from "zod";

/** Where this window's snapshot lives. Keyed by window label so a sibling
 *  window never reads (or overwrites) another's pane mapping, and so the
 *  crash-rebuilt window — same label — finds its own. */
function snapshotStorageKey(): string {
  return `pade.session-snapshot:${windows.label()}`;
}

/** One persisted session — `AgentSession` minus `initialPrompt`, which was
 *  already submitted into the live conversation (restoring it would make the
 *  re-attached terminal send it again). */
const SnapshotSession = z.object({
  id: z.string(),
  agent: Agent,
  cwd: z.string().optional(),
  branch: z.string().optional(),
  args: z.array(z.string()).optional(),
  // Kept so a session that re-attached after a reload can still be restarted
  // back into its own conversation when its MCP config later changes.
  conversationId: z.string().optional()
});

/** What a window needs to re-attach after a reload: the open project and the
 *  session/pane layout it was showing. */
export const SessionSnapshot = z.object({
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
}): void {
  if (project === "" || sessions.length === 0) {
    clearSessionSnapshot();
    return;
  }

  const snapshot: SessionSnapshot = {
    project,
    sessions: sessions.map(({ id, agent, cwd, branch, args, conversationId }) => ({
      id,
      agent,
      cwd,
      branch,
      args,
      conversationId
    })),
    paneIds: [...paneIds],
    activeId
  };
  localStorage.setItem(snapshotStorageKey(), JSON.stringify(snapshot));
}

export function clearSessionSnapshot(): void {
  localStorage.removeItem(snapshotStorageKey());
}

/** The persisted snapshot, or `null` when absent or malformed. Storage is a
 *  trust boundary like any other — the payload is zod-validated on the way in. */
export function readSessionSnapshot(): SessionSnapshot | null {
  const raw = localStorage.getItem(snapshotStorageKey());
  if (raw === null) {
    return null;
  }

  let decoded: unknown;
  try {
    decoded = JSON.parse(raw);
  } catch {
    return null;
  }

  const parsed = SessionSnapshot.safeParse(decoded);
  return parsed.success ? parsed.data : null;
}

/** The snapshot cut down to the sessions the backend still hosts — pure, so the
 *  intersection is unit-testable. Panes and the active id are pruned with it
 *  (a survivor always ends up shown); `null` when no session survived, which is
 *  exactly what a deliberate leave (PTYs killed) looks like. */
export function pruneToLive({ snapshot, liveIds }: {
  snapshot: SessionSnapshot;
  liveIds: ReadonlySet<string>;
}): SessionSnapshot | null {
  const sessions = snapshot.sessions.filter(session => liveIds.has(session.id));
  if (sessions.length === 0) {
    return null;
  }

  const survivingIds = new Set(sessions.map(session => session.id));
  const shownPaneIds = snapshot.paneIds.filter(id => survivingIds.has(id));
  const paneIds = shownPaneIds.length > 0 ? shownPaneIds : [sessions[0].id];

  const activeSurvived = snapshot.activeId !== null && survivingIds.has(snapshot.activeId);
  const activeId = activeSurvived ? snapshot.activeId : (paneIds.at(-1) ?? null);

  return {
    project: snapshot.project,
    sessions,
    paneIds,
    activeId
  };
}

/** The snapshot a reloaded window can actually re-attach to: the persisted pane
 *  mapping intersected with the backend's live-session roster (`pty_list`). A
 *  snapshot with nothing live behind it is stale — cleared and `null`. */
export async function restoreLiveSnapshot(): Promise<SessionSnapshot | null> {
  const snapshot = readSessionSnapshot();
  if (!snapshot) {
    return null;
  }

  const live = await pty.list();
  const restorable = pruneToLive({
    snapshot,
    liveIds: new Set(live.map(session => session.id))
  });
  if (!restorable) {
    clearSessionSnapshot();
  }

  return restorable;
}
