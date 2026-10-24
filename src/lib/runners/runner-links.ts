// Splitting a runner line's styled segments at its URLs, so the plain-HTML
// output makes the same links clickable that a terminal pane does.

import type { AnsiSegment } from "@/lib/terminal/ansi";
import { findUrls } from "@/lib/terminal/terminal-links";

type LinkedSegment = AnsiSegment & { url?: string };

/** The segments re-cut so every URL piece carries its whole `url`. URLs are
 *  found on the line's joined text, so one that crosses a colour change still
 *  opens in full from either piece. */
export function linkSegments(segments: AnsiSegment[]) {
  const urls = findUrls(segments.map(segment => segment.text).join(""));
  const linked: LinkedSegment[] = [];
  if (urls.length === 0) {
    linked.push(...segments);
    return linked;
  }

  let segmentStart = 0;
  for (const segment of segments) {
    const segmentEnd = segmentStart + segment.text.length;
    let cursor = segmentStart;
    for (const { url, start } of urls) {
      const urlEnd = start + url.length;
      const overlapStart = Math.max(start, cursor);
      const overlapEnd = Math.min(urlEnd, segmentEnd);
      if (overlapStart >= overlapEnd) {
        continue;
      }

      if (overlapStart > cursor) {
        linked.push({
          ...segment,
          text: segment.text.slice(cursor - segmentStart, overlapStart - segmentStart)
        });
      }

      linked.push({
        ...segment,
        text: segment.text.slice(overlapStart - segmentStart, overlapEnd - segmentStart),
        url
      });
      cursor = overlapEnd;
    }

    if (cursor < segmentEnd) {
      linked.push({
        ...segment,
        text: segment.text.slice(cursor - segmentStart)
      });
    }

    segmentStart = segmentEnd;
  }

  return linked;
}
