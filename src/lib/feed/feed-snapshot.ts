// Carry the Change Feed across a reload of its window.
//
// The backend replays nothing, so the feed lives only in the webview's memory and
// an F5 / dropped-HMR reload emptied it - while the watch that produced it kept
// running untouched in the backend. This module keeps the accumulated events in
// localStorage (keyed by window label, like the pane snapshot in session-restore,
// so a crash-rebuilt window under the same label finds them too) and hands them
// back only to the SAME watch: the backend's `armedAt` for this window must still
// match the one the events were collected under. A watch armed afresh - a later
// app run, a project switch, a re-armed root - never inherits another watch's
// changes, so liveness stays the backend's claim, never this snapshot's.

import { windows } from "@/lib/bridge";
import { ChangeEvent } from "@/lib/types";
import { readStoredJson, writeStoredJson } from "@/lib/util/stored-json";
import { z } from "zod";

function snapshotStorageKey() {
  return `pade.change-feed:${windows.label()}`;
}

/** The feed as one watch accumulated it: whose watch, armed when, and its events. */
const FeedSnapshot = z.object({
  root: z.string().min(1),
  armedAt: z.number(),
  events: z.array(ChangeEvent)
});
export type FeedSnapshot = z.infer<typeof FeedSnapshot>;

/** The watch a snapshot may be restored into - this window's live one. */
interface LiveWatch {
  root: string | null;
  armedAt: number | null;
}

export function saveFeedSnapshot(snapshot: FeedSnapshot) {
  writeStoredJson({
    key: snapshotStorageKey(),
    value: snapshot
  });
}

/** The persisted snapshot, or `null` when absent or malformed. */
export function readFeedSnapshot() {
  return readStoredJson({
    key: snapshotStorageKey(),
    schema: FeedSnapshot
  });
}

/** The snapshot's events when it was collected under the watch still live for
 *  this window (same root, same arming) on the project being opened, else none.
 *  The project check covers a switch, where the backend can still report the old
 *  project's watch for a moment before it re-roots. Pure, so the liveness rule is
 *  unit-testable. */
export function eventsForLiveWatch({ snapshot, watch, project }: {
  snapshot: FeedSnapshot | null;
  watch: LiveWatch;
  project: string;
}) {
  const isSameWatch = snapshot !== null
    && snapshot.root === project
    && snapshot.root === watch.root
    && snapshot.armedAt === watch.armedAt;
  return isSameWatch ? snapshot.events : [];
}
