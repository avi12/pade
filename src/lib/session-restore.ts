// Re-attach after an accidental WebView reload — the session-persistence seam.
//
// The pane-mapping snapshot lives in sessionStorage: it survives a reload of
// the same window (F5, a dropped HMR socket, a crash recovery) but dies with
// the window, so a deliberate app restart never resurrects agents the user
// meant to end. The backend's PtyState stays the sole authority on *liveness*:
// restore only re-attaches sessions `pty_list` still hosts, and a deliberate
// leave kills its PTYs first — so nothing survives the intersection and no
// separate "leave intent" flag is needed.

import { pty } from "@/lib/bridge";
import { Agent } from "@/lib/types";
import type { AgentSession } from "@/lib/types";
import { z } from "zod";

const SNAPSHOT_STORAGE_KEY = "pade.session-snapshot";

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
  sessionStorage.setItem(SNAPSHOT_STORAGE_KEY, JSON.stringify(snapshot));
}

export function clearSessionSnapshot(): void {
  sessionStorage.removeItem(SNAPSHOT_STORAGE_KEY);
}

/** The persisted snapshot, or `null` when absent or malformed. Storage is a
 *  trust boundary like any other — the payload is zod-validated on the way in. */
export function readSessionSnapshot(): SessionSnapshot | null {
  const raw = sessionStorage.getItem(SNAPSHOT_STORAGE_KEY);
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
