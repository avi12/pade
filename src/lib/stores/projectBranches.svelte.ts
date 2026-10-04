// Shared lazy branch lookup for project rows — the switcher's branch chip. Rides
// the visible-row batch (see visiblePathBatch): `vcs_branch_of` runs git once per
// path, so asking about only the rows on screen keeps an unbounded recent list
// as cheap to open as a short one.

import { vcs } from "@/lib/bridge";
import { createVisiblePathBatch } from "@/lib/stores/visiblePathBatch.svelte";

const projectBranches = createVisiblePathBatch<string>({
  load: paths => vcs.branchOf(paths),
  cacheLimit: 256
});

/** Cached branch: `undefined` while unresolved, `null` for no repo / detached HEAD. */
export const projectBranch = projectBranches.value;

/** Shared attachment that looks a project's branch up once its row is visible. */
export const observeProjectBranch = projectBranches.observe;

/** Branches move under us (a checkout in any project), so the switcher drops its
 *  cache each time it opens and re-asks for the rows on screen. */
export const invalidateProjectBranches = projectBranches.invalidate;
