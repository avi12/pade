// The Change Feed's opt-in file-type filter: the per-extension tally its popover
// lists, the label its chip reads, and the narrowing it applies. An empty
// selection means "no filter - show every type".

import { fileExtension } from "@/lib/feed/file-type";
import type { ChangeEvent } from "@/lib/types";
import { formatCount } from "@/lib/util/format";

interface ExtensionCount {
  extension: string;
  count: number;
}

/** The per-extension tally over the feed, sorted by count, then name, so the
 *  busiest types lead and the order is stable. */
export function countExtensions(events: ChangeEvent[]) {
  const counts: Record<string, number> = {};
  for (const event of events) {
    const extension = fileExtension(event.path);
    counts[extension] = (counts[extension] ?? 0) + 1;
  }

  return Object.entries(counts)
    .map(([extension, count]) => ({
      extension,
      count
    }))
    .sort((first, second) => second.count - first.count || first.extension.localeCompare(second.extension));
}

/** Whether every listed type is ticked - the popover's "Select all" then reads
 *  "Clear". An empty list is never "all selected". */
export function isEveryTypeSelected({ counts, selected }: {
  counts: ExtensionCount[];
  selected: ReadonlySet<string>;
}) {
  return counts.length > 0 && counts.every(item => selected.has(item.extension));
}

/** The chip's reading of the selection: "All types" when nothing (or
 *  everything) is ticked, else how many of the listed types are kept. */
export function typeFilterLabel({ counts, selected }: {
  counts: ExtensionCount[];
  selected: ReadonlySet<string>;
}) {
  const isNoFilterActive = selected.size === 0 || isEveryTypeSelected({
    counts,
    selected
  });
  if (isNoFilterActive) {
    return "All types";
  }

  return `${formatCount(selected.size)} of ${formatCount(counts.length)} types`;
}

/** The events the selection keeps - all of them when nothing is ticked. */
export function filterByExtension({ events, selected }: {
  events: ChangeEvent[];
  selected: ReadonlySet<string>;
}) {
  if (selected.size === 0) {
    return events;
  }

  return events.filter(event => selected.has(fileExtension(event.path)));
}
