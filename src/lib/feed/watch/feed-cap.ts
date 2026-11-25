import type { ChangeEvent } from "@/lib/types/feed";

/** How many changes a project's feed keeps of each kind - the ones the
 *  project keeps and the ones its `.gitignore` ignores - so a busy agent
 *  session can't grow it unbounded. */
const MAXIMUM_EVENTS_PER_KIND = 300;

/** A newest-first feed cut to its cap. Ignored and kept changes are counted
 *  apart: a tool churning ignored files (a browser profile, a build cache)
 *  must never push the changes the feed shows out of it. */
export function cappedFeed(events: readonly ChangeEvent[]) {
  let keptCount = 0;
  let ignoredCount = 0;
  const capped: ChangeEvent[] = [];
  for (const event of events) {
    const count = event.isIgnored ? ignoredCount : keptCount;
    if (count >= MAXIMUM_EVENTS_PER_KIND) {
      continue;
    }

    capped.push(event);

    if (event.isIgnored) {
      ignoredCount += 1;
    } else {
      keptCount += 1;
    }
  }

  return capped;
}
