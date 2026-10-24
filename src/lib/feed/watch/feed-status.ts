// What the Change Feed says when it has nothing to show. The list being empty
// has three very different causes - nothing has changed yet, the watch is armed
// on another project, or there is no watch at all - and the panel used to word
// all three as "Waiting for edits". This turns the backend's `WatchStatus` into
// the sentence that names which one it is (pure, so it is tested rather than
// eyeballed).

import type { WatchStatus } from "@/lib/types";
import { formatAge, formatCount } from "@/lib/util/format";
import { baseName, normalizePath } from "@/lib/util/paths";

interface FeedWatchSummary {
  /** The headline: what state the feed is in. */
  headline: string;
  /** The evidence behind it - what is watched, since when, what was hidden. */
  detail: string;
  /** No live watch on the open project: the panel offers to arm it again. */
  isStalled: boolean;
}

/** The empty feed's honest self-description for `project`, given the window's
 *  live watch `status` (`null` before the first read). */
export function watchSummary({ status, project, now }: {
  status: WatchStatus | null;
  project: string;
  now: number;
}) {
  const name = baseName(project) || project;
  if (status === null) {
    return {
      headline: "Waiting for edits",
      detail: `Checking what ${name} is watching…`,
      isStalled: false
    } satisfies FeedWatchSummary;
  }

  if (status.root === null) {
    return {
      headline: "Not watching this project",
      detail: `No file watch is armed on ${name}, so nothing can reach the feed`,
      isStalled: true
    } satisfies FeedWatchSummary;
  }

  const isWatchingThisProject = normalizePath(status.root) === normalizePath(project);
  if (!isWatchingThisProject) {
    return {
      headline: "Watching a different project",
      detail: `The watch is armed on ${baseName(status.root) || status.root}, not ${name}`,
      isStalled: true
    } satisfies FeedWatchSummary;
  }

  const since = status.armedAt === null
    ? "since it opened"
    : `for ${formatAge({
      stamp: status.armedAt,
      now
    })}`;
  const hidden = status.ignored > 0
    ? `. ${formatCount(status.ignored)} hidden by this project's ignore rules`
    : "";
  return {
    headline: "No changes yet",
    detail: `Watching ${name} ${since} - the feed starts from the moment a project opens, and shows nothing from before it${hidden}`,
    isStalled: false
  } satisfies FeedWatchSummary;
}
