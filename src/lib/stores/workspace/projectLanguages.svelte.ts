// Shared lazy language-share lookup for project rows - the GitHub-style language
// bar. Rides the visible-row batch (see visiblePathBatch), so only rows on screen
// pay for the source census behind it, and keeps it live: the rows on screen -
// and the projects the window follows, always - are watched (their git folders
// and their working trees, ignore rules applied), and a project whose files
// change is counted again once the burst settles.

import { Command, EventType, onMessage, sendMessage } from "@/lib/ipc/messaging";
import { isFollowedProject, watchedProjectPaths } from "@/lib/stores/workspace/followed-projects";
import { createVisiblePathBatch, PROJECT_ROW_CACHE_LIMIT } from "@/lib/stores/workspace/visiblePathBatch.svelte";
import type { LanguageShare } from "@/lib/types";

/** Rows scroll past in bursts; the watch follows them once the list settles. */
const WATCH_SETTLE_MS = 300;
/** A git operation writes the index several times, and an agent writes files
 *  in bursts; count once the project has been quiet this long. */
const RECOUNT_SETTLE_MS = 1_500;

let watchTimer: ReturnType<typeof setTimeout> | undefined;
// eslint-disable-next-line svelte/prefer-svelte-reactivity -- debounce bookkeeping, never rendered
const recountTimers = new Map<string, ReturnType<typeof setTimeout>>();
let isListening = false;
let visiblePaths: string[] = [];
/** The projects the window follows and what to do when one's languages change. */
let followed: {
  paths: string[];
  onchange: (path: string) => Promise<void>;
} = {
  paths: [],
  async onchange() {}
};

async function watchProjects(paths: string[]) {
  try {
    await sendMessage(Command.watchLanguages, { paths });
  } catch (error) {
    console.warn("pade: could not watch the projects for language changes", error);
  }
}

/** Re-arm the backend watch on the followed projects plus the rows on screen,
 *  once the list has settled. */
function rearmWatch() {
  clearTimeout(watchTimer);
  watchTimer = setTimeout(() => watchProjects(
    watchedProjectPaths({
      followed: followed.paths,
      visible: visiblePaths
    })
  ), WATCH_SETTLE_MS);
}

/** Count a project again; when it is a followed one and its mix moved, tell the
 *  window - an unchanged count after a commit asks nothing more of anyone. The
 *  old bar stays up until the new count lands, so a row on screen re-flows its
 *  segments in place rather than blanking. */
async function recount(path: string) {
  const before = JSON.stringify(projectLanguages.value(path));
  const after = JSON.stringify(await projectLanguages.refresh(path));
  const isFollowed = isFollowedProject({
    followed: followed.paths,
    path
  });
  if (isFollowed && before !== after) {
    await followed.onchange(path);
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

/** Keep these projects counted and watched for as long as the window works in
 *  them - not only while their rows are on screen - and call `onchange` with a
 *  project whose language mix moved (the editor that suits it may have
 *  changed). Replaces the previous list; an empty list stops. */
export function followProjects({ paths, onchange }: {
  paths: string[];
  onchange: (path: string) => Promise<void>;
}) {
  ensureListening();
  followed = {
    paths,
    onchange
  };

  for (const path of paths) {
    projectLanguages.request(path);
  }

  rearmWatch();
}

/** {@link followProjects} for the window's one own project. An empty path stops. */
export function followOwnProject({ path, onchange }: {
  path: string;
  onchange: () => Promise<void>;
}) {
  followProjects({
    paths: path === "" ? [] : [path],
    onchange
  });
}

/** Cached language shares: `undefined` while unresolved, `null` when the
 *  project has no countable source. */
export const projectLanguageShares = projectLanguages.value;

/** Shared attachment that measures a project only when its row becomes visible. */
export const observeProjectLanguages = projectLanguages.observe;

/** Re-check every cached project as its row shows, keeping the old bar until
 *  the answer lands - cheap, since the backend recounts only a project whose
 *  git index moved, or whose working tree a watch saw change, since its last
 *  count. */
export const revalidateProjectLanguages = projectLanguages.revalidate;
