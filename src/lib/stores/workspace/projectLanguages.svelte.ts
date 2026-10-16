// Shared lazy language-share lookup for project rows - the GitHub-style language
// bar. Rides the visible-row batch (see visiblePathBatch), so only rows on screen
// pay for the source census behind it, and keeps it live: while rows are on
// screen their `.git` folders are watched, and a row whose tracked files change
// is counted again.

import { Command, EventType, onMessage, sendMessage } from "@/lib/ipc/messaging";
import { createVisiblePathBatch, PROJECT_ROW_CACHE_LIMIT } from "@/lib/stores/workspace/visiblePathBatch.svelte";
import type { LanguageShare } from "@/lib/types";

/** Rows scroll past in bursts; the watch follows them once the list settles. */
const WATCH_SETTLE_MS = 300;
/** A git operation writes the index several times; count once it is done. */
const RECOUNT_SETTLE_MS = 1_500;

let watchTimer: ReturnType<typeof setTimeout> | undefined;
// eslint-disable-next-line svelte/prefer-svelte-reactivity -- debounce bookkeeping, never rendered
const recountTimers = new Map<string, ReturnType<typeof setTimeout>>();
let isListening = false;

async function watchRows(paths: string[]) {
  try {
    await sendMessage(Command.watchLanguages, { paths });
  } catch (error) {
    console.warn("pade: could not watch the visible projects for language changes", error);
  }
}

async function listenForChanges() {
  isListening = true;
  await onMessage(EventType.languagesChanged, ({ data: path }) => {
    clearTimeout(recountTimers.get(path));
    recountTimers.set(
      path, setTimeout(async () => {
        recountTimers.delete(path);
        await projectLanguages.refresh(path);
      }, RECOUNT_SETTLE_MS)
    );
  });
}

const projectLanguages = createVisiblePathBatch<LanguageShare[]>({
  load: paths => sendMessage(Command.ideProjectLanguages, { paths }),
  cacheLimit: PROJECT_ROW_CACHE_LIMIT,
  onVisiblePathsChange(paths) {
    if (!isListening) {
      listenForChanges();
    }

    clearTimeout(watchTimer);
    watchTimer = setTimeout(() => watchRows(paths), WATCH_SETTLE_MS);
  }
});

/** Cached language shares: `undefined` while unresolved, `null` when the
 *  project has no countable source. */
export const projectLanguageShares = projectLanguages.value;

/** Shared attachment that measures a project only when its row becomes visible. */
export const observeProjectLanguages = projectLanguages.observe;

/** Re-check every cached project as its row shows, keeping the old bar until
 *  the answer lands - cheap, since the backend recounts only a project whose
 *  git index moved since its last count. */
export const revalidateProjectLanguages = projectLanguages.revalidate;
