import { parseDiff, unifiedDiff } from "@/lib/code/diff";
import type { FeedDiff } from "@/lib/types";

/** Turn the backend's session-baseline pair into diff lines to render. Normally
 *  that's the baseline→current diff. But a file the watcher first saw a beat
 *  AFTER it was written baselines equal to its current content - the write beat
 *  the first watch poll - so that diff is empty even though the card counts real
 *  growth ("Grew by N lines"). Rather than strand it on "No preview available",
 *  fall back to previewing the whole current file: everything a file created this
 *  session holds IS new, so the full content is the honest preview. A deletion
 *  (empty `after`) keeps its real removal diff - no fallback. */
export function previewLines(preview: FeedDiff | null) {
  if (!preview) {
    return [];
  }

  const changed = parseDiff(
    unifiedDiff({
      before: preview.before,
      after: preview.after
    })
  );
  if (changed.length > 0 || preview.after.length === 0) {
    return changed;
  }

  return parseDiff(
    unifiedDiff({
      before: "",
      after: preview.after
    })
  );
}
