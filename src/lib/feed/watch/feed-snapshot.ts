// Carry the Change Feed across a reload of its window.
//
// The backend replays nothing, so the feed lives only in the webview's memory and
// an F5 / dropped-HMR reload emptied it - while the watches that produced it kept
// running untouched in the backend. This module keeps every held project's
// accumulated events in localStorage (keyed by window label, like the pane
// snapshot in session-restore, so a crash-rebuilt window under the same label
// finds them too) and hands each project's back only to the SAME watch: the
// backend's `armedAt` for that root must still match the one its events were
// collected under. A watch armed afresh - a later app run, a project dropped and
// held again - never inherits another watch's changes, so liveness stays the
// backend's claim, never this snapshot's.

import { windowLabel } from "@/lib/ipc/platform";
import { ChangeEvent } from "@/lib/types";
import type { WatchStatus } from "@/lib/types";
import { readStoredJson, writeStoredJson } from "@/lib/util/data/stored-json";
import { z } from "zod";

function snapshotStorageKey() {
  return `pade.change-feed:${windowLabel()}`;
}

/** One project's feed as its watch accumulated it: whose watch, armed when,
 *  and its events. */
const ProjectFeed = z.object({
  root: z.string().min(1),
  armedAt: z.number(),
  events: z.array(ChangeEvent)
});

/** Every held project's feed, one entry per root. */
const FeedSnapshot = z.object({
  feeds: z.array(ProjectFeed)
});
export type FeedSnapshot = z.infer<typeof FeedSnapshot>;

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

/** The events the snapshot holds for `root` when they were collected under the
 *  watch still live on it (`watch_status { root }` reports the same arming),
 *  else none. Pure, so the liveness rule is unit-testable. */
export function eventsForLiveWatch({ snapshot, root, watch }: {
  snapshot: FeedSnapshot | null;
  root: string;
  watch: Pick<WatchStatus, "armedAt">;
}) {
  const saved = snapshot?.feeds.find(feed => feed.root === root);
  const isSameWatch = saved !== undefined && watch.armedAt !== null && saved.armedAt === watch.armedAt;
  return isSameWatch ? saved.events : [];
}
