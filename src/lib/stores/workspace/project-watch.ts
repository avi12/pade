// The window's file watch: one backend watch per project the window holds a
// session of, so an agent working in a project behind the front one still
// reaches that project's Change Feed, task catalog and auto-namer. The backend
// takes the whole set every time (`watch_projects`) and arms or drops the
// difference; this module owns the set the window last asked for.

import { Command, sendMessage } from "@/lib/ipc/messaging";

const ROOT_SEPARATOR = "\n";

let heldRoots: string[] = [];
let heldKey: string | undefined;
const armedListeners = new Set<() => Promise<void>>();

/** Run `listener` each time a watch request settles - what a reader of
 *  `watch_status` needs, since the window arms its watches after it mounts.
 *  Returns the unsubscribe. */
export function onWatchesArmed(listener: () => Promise<void>) {
  armedListeners.add(listener);
  return () => {
    armedListeners.delete(listener);
  };
}

async function askToWatch(roots: string[]) {
  try {
    await sendMessage(Command.watchProjects, { roots });
  } finally {
    for (const listener of armedListeners) {
      await listener();
    }
  }
}

/** The projects a window holds: every session's project, plus the front one
 *  while it has no session yet (just opened, its first agent still launching).
 *  Sorted and unique, so the same set always reads the same. */
export function heldProjects({ sessionProjects, frontProject }: {
  sessionProjects: string[];
  frontProject: string;
}) {
  const roots = new Set([...sessionProjects, frontProject]);
  roots.delete("");
  return [...roots].sort();
}

/** Watch exactly `roots` (see `heldProjects`). Asks the backend only when the
 *  set changed, so a session merely changing status never re-sends it - unless
 *  the last ask failed, which the next one retries. */
export async function watchHeldProjects(roots: string[]) {
  const key = roots.join(ROOT_SEPARATOR);
  if (key === heldKey) {
    return;
  }

  heldKey = key;
  heldRoots = roots;
  try {
    await askToWatch(roots);
  } catch (error) {
    heldKey = undefined;
    throw error;
  }
}

/** Ask for the held set again - the backend arms any of it that is not watched,
 *  so a project whose watch failed to arm gets another try. */
export async function rewatchHeldProjects() {
  await askToWatch(heldRoots);
}
