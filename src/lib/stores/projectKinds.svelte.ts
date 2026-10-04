// Shared lazy project-kind detection for project rows — the language icon. Rides
// the visible-row batch (see visiblePathBatch), so only rows on screen are probed.

import { ide } from "@/lib/bridge";
import { createVisiblePathBatch } from "@/lib/stores/visiblePathBatch.svelte";

const projectKinds = createVisiblePathBatch<string>({
  load: paths => ide.projectKinds(paths),
  cacheLimit: 256
});

/** Queue one path through the shared batch. Exposed separately from the DOM
 * attachment so non-visual consumers and unit tests use the identical loader. */
export const requestProjectKind = projectKinds.request;

/** Cached project kind: `undefined` while unresolved, `null` when probed empty. */
export const projectKind = projectKinds.value;

/** Probe one project immediately and publish through the shared cache. This is
 * used by non-visual evidence flows that must know when a marker first exists. */
export const refreshProjectKind = projectKinds.refresh;

/** Shared attachment that detects a project only when its row becomes visible. */
export const observeProjectKind = projectKinds.observe;
