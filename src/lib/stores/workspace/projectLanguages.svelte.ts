// Shared lazy language-share lookup for project rows - the GitHub-style language
// bar. Rides the visible-row batch (see visiblePathBatch), so only rows on screen
// pay for the source census behind it.

import { Command, sendMessage } from "@/lib/ipc/messaging";
import { createVisiblePathBatch, PROJECT_ROW_CACHE_LIMIT } from "@/lib/stores/workspace/visiblePathBatch.svelte";
import type { LanguageShare } from "@/lib/types";

const projectLanguages = createVisiblePathBatch<LanguageShare[]>({
  load: paths => sendMessage(Command.ideProjectLanguages, { paths }),
  cacheLimit: PROJECT_ROW_CACHE_LIMIT
});

/** Cached language shares: `undefined` while unresolved, `null` when the
 *  project has no countable source. */
export const projectLanguageShares = projectLanguages.value;

/** Shared attachment that measures a project only when its row becomes visible. */
export const observeProjectLanguages = projectLanguages.observe;
