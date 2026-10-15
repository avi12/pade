// Pure layout math behind the workspace's tabs and split panes: which sessions
// exist, which are shown side by side (`paneIds`, always a subset of the
// sessions) and which one is active. App.svelte owns that reactive state and
// assigns what these return; the decisions themselves stay DOM-free and
// unit-tested, next to the index math in `@/lib/reorder`.

import { DropSide, paneDropSide, paneInsertIndex } from "@/lib/reorder";
import type { AgentSession } from "@/lib/types";

/** The workspace's tab and pane state, as App.svelte holds it. */
export interface PaneLayout {
  sessions: AgentSession[];
  paneIds: string[];
  activeId: string | null;
}

/** A shown pane and where it sits on screen. */
interface MeasuredPane {
  id: string;
  rectangle: Pick<DOMRect, "left" | "right" | "top" | "bottom" | "width">;
}

/** The shown pane under a point, and which half of it. The live highlight and
 *  the actual drop both ask this, so the two never disagree. A collapsed pane
 *  (zero width) is never a target. */
export function paneAtPoint({ panes, x, y }: {
  panes: readonly MeasuredPane[];
  x: number;
  y: number;
}) {
  const hit = panes.find(({ rectangle }) => rectangle.width > 0
    && x >= rectangle.left && x <= rectangle.right
    && y >= rectangle.top && y <= rectangle.bottom);
  if (!hit) {
    return null;
  }

  return {
    id: hit.id,
    side: paneDropSide({
      pointerX: x,
      left: hit.rectangle.left,
      width: hit.rectangle.width
    })
  };
}

/** The pane order after a tab is dropped on one side of `targetId`; a tab that
 *  is already shown moves rather than duplicating. */
export function paneIdsAfterSplit({ paneIds, draggedId, targetId, side }: {
  paneIds: string[];
  draggedId: string;
  targetId: string;
  side: DropSide;
}) {
  const base = paneIds.filter(id => id !== draggedId);
  const iInsert = paneInsertIndex({
    paneIds,
    draggedId,
    targetId,
    side
  });
  return [...base.slice(0, iInsert), draggedId, ...base.slice(iInsert)];
}

/** The tab strip shows a prefix of `sessions` (the overflow tail stays put), so
 *  a reorder of the visible pills slots their new order back into the positions
 *  the visible set held. */
export function sessionsAfterReorder({ sessions, orderedIds }: {
  sessions: AgentSession[];
  orderedIds: string[];
}) {
  const bySessionId = new Map(sessions.map(session => [session.id, session] as const));
  const visibleIds = new Set(orderedIds);
  const queue = [...orderedIds];
  return sessions.map(session => {
    if (!visibleIds.has(session.id)) {
      return session;
    }

    const nextId = queue.shift();
    return (nextId ? bySessionId.get(nextId) : undefined) ?? session;
  });
}

/** The index `delta` steps from `index`, wrapping at both ends. */
export function wrappedIndex({ index, delta, count }: {
  index: number;
  delta: number;
  count: number;
}) {
  return (index + delta + count) % count;
}

/** The layout once one pane leaves the split (its session keeps running). The
 *  active pane falls back to the last one still shown. */
export function layoutWithoutPane({ layout, id }: {
  layout: PaneLayout;
  id: string;
}) {
  const paneIds = layout.paneIds.filter(paneId => paneId !== id);
  const activeId = layout.activeId === id ? (paneIds.at(-1) ?? null) : layout.activeId;
  return {
    ...layout,
    paneIds,
    activeId
  };
}

/** The layout once a session is gone for good. The active session falls back to
 *  the last shown pane, else the last tab, and a split emptied by it shows that
 *  fallback, so the terminal area never goes blank while a session remains. */
export function layoutWithoutSession({ layout, id }: {
  layout: PaneLayout;
  id: string;
}) {
  const sessions = layout.sessions.filter(session => session.id !== id);
  const paneIds = layout.paneIds.filter(paneId => paneId !== id);
  const activeId = layout.activeId === id
    ? (paneIds.at(-1) ?? sessions.at(-1)?.id ?? null)
    : layout.activeId;
  const isSplitEmptied = paneIds.length === 0 && activeId !== null;
  return {
    sessions,
    paneIds: isSplitEmptied ? [activeId] : paneIds,
    activeId
  };
}

/** The layout once several sessions leave together (a relocation ending them).
 *  The active session falls back to the last remaining tab. */
export function layoutWithoutSessions({ layout, ids }: {
  layout: PaneLayout;
  ids: ReadonlySet<string>;
}) {
  const sessions = layout.sessions.filter(session => !ids.has(session.id));
  const isActiveRemoved = layout.activeId !== null && ids.has(layout.activeId);
  return {
    sessions,
    paneIds: layout.paneIds.filter(id => !ids.has(id)),
    activeId: isActiveRemoved ? (sessions.at(-1)?.id ?? null) : layout.activeId
  };
}
