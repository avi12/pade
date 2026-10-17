// Shared lazy language-share lookup for project rows - the GitHub-style language
// bar. Rides the visible-row batch (see visiblePathBatch), so only rows on screen
// pay for the source census behind it, and keeps it live: the git folders of the
// rows on screen - and of the window's own project, always - are watched, and a
// project whose tracked files change is counted again.

import { Command, EventType, onMessage, sendMessage } from "@/lib/ipc/messaging";
import { createVisiblePathBatch, PROJECT_ROW_CACHE_LIMIT } from "@/lib/stores/workspace/visiblePathBatch.svelte";
import type { LanguageShare } from "@/lib/types";
import { normalizePath } from "@/lib/util/paths";

/** Rows scroll past in bursts; the watch follows them once the list settles. */
const WATCH_SETTLE_MS = 300;
/** A git operation writes the index several times; count once it is done. */
const RECOUNT_SETTLE_MS = 1_500;

let watchTimer: ReturnType<typeof setTimeout> | undefined;
// eslint-disable-next-line svelte/prefer-svelte-reactivity -- debounce bookkeeping, never rendered
const recountTimers = new Map<string, ReturnType<typeof setTimeout>>();
let isListening = false;
let visiblePaths: string[] = [];
/** The window's own project and what to do when its languages change. */
let ownProject: {
  path: string;
  onchange: () => Promise<void>;
} | null = null;

async function watchRows(paths: string[]) {
  try {
    await sendMessage(Command.watchLanguages, { paths });
  } catch (error) {
    console.warn("pade: could not watch the projects for language changes", error);
  }
}

/** Re-arm the backend watch on the rows on screen plus the window's own project,
 *  once the list has settled. */
function rearmWatch() {
  const paths = ownProject ? [ownProject.path, ...visiblePaths] : visiblePaths;
  clearTimeout(watchTimer);
  watchTimer = setTimeout(() => watchRows([...new Set(paths)]), WATCH_SETTLE_MS);
}

/** Count a project again; when it is the window's own and its mix moved, tell
 *  the window - an unchanged count after a commit asks nothing more of anyone. */
async function recount(path: string) {
  const before = JSON.stringify(projectLanguages.value(path));
  const after = JSON.stringify(await projectLanguages.refresh(path));
  const isOwnProject = ownProject !== null && normalizePath(ownProject.path) === normalizePath(path);
  if (isOwnProject && before !== after) {
    await ownProject?.onchange();
  }
}

async function listenForChanges() {
  isListening = true;
  await onMessage(EventType.languagesChanged, ({ data: path }) => {
    clearTimeout(recountTimers.get(path));
    recountTimers.set(
      path, setTimeout(async () => {
        recountTimers.delete(path);
        await recount(path);
      }, RECOUNT_SETTLE_MS)
    );
  });
}

function ensureListening() {
  if (!isListening) {
    listenForChanges();
  }
}

const projectLanguages = createVisiblePathBatch<LanguageShare[]>({
  load: paths => sendMessage(Command.ideProjectLanguages, { paths }),
  cacheLimit: PROJECT_ROW_CACHE_LIMIT,
  onVisiblePathsChange(paths) {
    ensureListening();
    visiblePaths = paths;
    rearmWatch();
  }
});

/** Keep the window's own project counted and watched for as long as it is open
 *  - not only while its row is on screen - and call `onchange` when its language
 *  mix moves (the editor that suits it may have changed). An empty path stops. */
export function followOwnProject({ path, onchange }: {
  path: string;
  onchange: () => Promise<void>;
}) {
  ensureListening();
  ownProject = null;

  if (path !== "") {
    ownProject = {
      path,
      onchange
    };
    projectLanguages.request(path);
  }

  rearmWatch();
}

/** Cached language shares: `undefined` while unresolved, `null` when the
 *  project has no countable source. */
export const projectLanguageShares = projectLanguages.value;

/** Shared attachment that measures a project only when its row becomes visible. */
export const observeProjectLanguages = projectLanguages.observe;

/** Re-check every cached project as its row shows, keeping the old bar until
 *  the answer lands - cheap, since the backend recounts only a project whose
 *  git index moved since its last count. */
export const revalidateProjectLanguages = projectLanguages.revalidate;
