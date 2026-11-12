// Which projects a window's language watch covers - pure, so the store that
// arms the watch stays a thin shell around it.

import { normalizePath } from "@/lib/util/paths";

/** The projects to watch: the ones the window follows, then the rows on
 *  screen - each project once, however each list spelled its path. */
export function watchedProjectPaths({ followed, visible }: {
  followed: string[];
  visible: string[];
}) {
  const seen = new Set<string>();
  const paths: string[] = [];
  for (const path of [...followed, ...visible]) {
    const key = normalizePath(path);
    if (seen.has(key)) {
      continue;
    }

    seen.add(key);
    paths.push(path);
  }
  return paths;
}

/** Whether `path` names one of the followed projects. */
export function isFollowedProject({ followed, path }: {
  followed: string[];
  path: string;
}) {
  const key = normalizePath(path);
  return followed.some(followedPath => normalizePath(followedPath) === key);
}
