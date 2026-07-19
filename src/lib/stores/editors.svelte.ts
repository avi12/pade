// The one frontend home for "which editor is this project's?" (DRY/SSOT).
// Every surface that shows or launches the project's editor — the top-bar
// IdeMenu and the Change Feed's reveal — reads the ranked list from here, so
// the two can never drift apart (a top bar saying "VS Code" while the feed
// reveals in WebStorm). The backend `ide_suggest` owns the actual resolution
// (explicit per-project choice → rules → coverage ranking); this store owns the
// single cached fetch per project and the choose-editor write-through.

import { ide } from "@/lib/bridge";
import { adoptPrefs } from "@/lib/prefs.svelte";
import type { Ide, Settings } from "@/lib/types";
import { SvelteMap } from "svelte/reactivity";

/** Ranked editors per project directory — `editorsFor(project)[0]` is *the*
 *  project's editor, mirroring `ide_suggest`'s array-order contract. */
const rankedByProject = new SvelteMap<string, Ide[]>();

/** One in-flight suggestion per project: simultaneous callers (the IdeMenu and
 *  the Change Feed mounting together) coalesce onto a single backend census
 *  instead of each running their own. */
const inFlight = new Map<string, Promise<void>>();

/** Identity of the newest fetch per project. A superseded fetch (an explicit
 *  pick landed while its census ran) finds a different token here when it
 *  resolves and discards its pre-pick ranking instead of publishing it. */
const currentFetchToken = new Map<string, symbol>();

/** The ranked editors for `project` — reactive; empty until resolved. */
export function editorsFor(project: string): Ide[] {
  return rankedByProject.get(project) ?? [];
}

async function fetchEditors({ project, token }: {
  project: string;
  token: symbol;
}): Promise<void> {
  let ranked: Ide[];
  try {
    ranked = await ide.suggest(project);
  } catch {
    ranked = [];
  }

  const isSuperseded = currentFetchToken.get(project) !== token;
  if (isSuperseded) {
    return;
  }

  rankedByProject.set(project, ranked);
  inFlight.delete(project);
}

/** Re-resolve the project's editors now (project switch, app visibility
 *  regained, an explicit choice saved). Coalesces with a fetch already in
 *  flight for the same project. */
export function refreshEditors(project: string): Promise<void> {
  const running = inFlight.get(project);
  if (running) {
    return running;
  }

  const token = Symbol(project);
  currentFetchToken.set(project, token);
  const fetch = fetchEditors({ project, token });
  inFlight.set(project, fetch);
  return fetch;
}

/** Resolve only when nothing is cached yet — a remounting panel (the Change
 *  Feed re-enters the DOM on every side-panel switch) reuses the cached list
 *  instead of re-running the project census. */
export function ensureEditors(project: string): Promise<void> {
  if (rankedByProject.has(project)) {
    return Promise.resolve();
  }

  return refreshEditors(project);
}

/** Persist an explicit editor pick for the project, then re-resolve, so every
 *  surface reading this store sees the choice win at once. */
export async function chooseEditor({ project, editorId }: {
  project: string;
  editorId: string;
}): Promise<void> {
  let settings: Settings;
  try {
    settings = await ide.choose({ cwd: project, id: editorId });
  } catch {
    // The pick didn't persist; the current ranking is still valid as-is.
    return;
  }
  // The shared prefs store must learn the persisted pick right away: the next
  // `updatePrefs` save round-trips that store's whole set, so a copy without
  // this `ideProjectChoices` entry would silently erase the pick on disk.
  adoptPrefs(settings.prefs);
  // A census already running read the prefs before the pick landed — dropping
  // its in-flight entry makes the refresh below start a fresh fetch (with a
  // new token) instead of coalescing onto the stale pre-pick one.
  inFlight.delete(project);
  await refreshEditors(project);
}
