// Persistent accumulation of the Change Feed's events (SoC: cross-component state
// lives in lib/stores). ChangeFeed mounts and unmounts as the side panel switches,
// so keeping the feed in component-local state emptied it on every switch — the
// backend keeps no replay. This module owns the single live subscription to the
// feed stream, started once and never torn down, so events keep arriving while the
// panel is closed and survive its remount.

import { feed } from "@/lib/bridge";
import { eventsForLiveWatch, readFeedSnapshot, saveFeedSnapshot } from "@/lib/feed-snapshot";
import { baseName } from "@/lib/paths";
import type { ChangeEvent, WatchStatus } from "@/lib/types";

// Newest first. Capped so a busy agent session can't grow this unbounded.
const CAP = 300;

// Editor/tool scratch files that churn during an atomic save (write to a temp
// name, then rename over the target) — noise, not real changes. Match the shapes
// the feed sees: a `.tmp.` infix, a `_tmp_` scratch name, a vim swap, a trailing
// `~` backup, or a long numeric atomic-save suffix.
const TEMP_FILE = /^_tmp_|\.tmp\.|\.sw[a-z]$|~$|\.\d{7,}$/i;

/** The live feed — newest-first and capped. Reactive, so the panel re-renders as
 *  events land whether or not it is currently mounted. */
export const feedStore = $state<{ events: ChangeEvent[] }>({ events: [] });

// The project the accumulated events belong to; a switch resets the feed.
let currentProject: string | null = null;

// The stream is subscribed exactly once for the process lifetime.
let subscribed = false;

// The backend watch the accumulated events came from, learned on retarget. A
// reload saves the feed under it and restores only into that same watch (see
// feed-snapshot).
let liveWatch: WatchStatus | null = null;

// Hand the feed to the reloaded page. `pagehide` is the last synchronous moment
// before the webview drops this page, so the save happens once per unload rather
// than re-serializing up to CAP events on every change.
function saveFeedOnUnload(): void {
  if (!liveWatch?.root || liveWatch.armedAt === null) {
    return;
  }

  saveFeedSnapshot({
    root: liveWatch.root,
    armedAt: liveWatch.armedAt,
    events: feedStore.events
  });
}

/** Subscribe once to the backend feed stream. Idempotent — a second call is a
 *  no-op — and never unsubscribed, so events accumulate across panel remounts. */
async function startFeedSubscription(): Promise<void> {
  if (subscribed) {
    return;
  }

  subscribed = true;
  addEventListener("pagehide", saveFeedOnUnload);
  try {
    await feed.onChange(event => {
      const isScratchFile = TEMP_FILE.test(baseName(event.path));
      if (isScratchFile) {
        return;
      }

      feedStore.events = [event, ...feedStore.events].slice(0, CAP);
    });
    // The ignore rules are live: editing (or creating, or deleting) a .gitignore
    // — and a mid-session `git init` — re-filters what the feed already shows, so
    // a path the project just started ignoring drops out instead of lingering.
    // The backend is the one authority on "ignored" (git's own rules in a repo,
    // the root .gitignore + tech inference otherwise); the store only asks.
    await feed.onIgnoreChanged(async () => {
      const paths = [...new Set(feedStore.events.map(event => event.path))];
      if (paths.length === 0) {
        return;
      }

      const nowIgnored = new Set(await feed.ignored(paths));
      if (nowIgnored.size > 0) {
        feedStore.events = feedStore.events.filter(event => !nowIgnored.has(event.path));
      }
    });
  } catch {
    // Re-arm on a failed subscribe so a later retarget can try again.
    subscribed = false;
    removeEventListener("pagehide", saveFeedOnUnload);
  }
}

// Learn the watch now feeding `project`, and — after a reload — put back the
// events that same watch had already surfaced, behind any that arrived since.
async function adoptLiveWatch(project: string): Promise<void> {
  try {
    liveWatch = await feed.status();
  } catch {
    liveWatch = null;
    return;
  }

  const restored = eventsForLiveWatch({
    snapshot: readFeedSnapshot(),
    watch: liveWatch,
    project
  });
  const isStillThisProject = project === currentProject;
  if (restored.length === 0 || !isStillThisProject) {
    return;
  }

  const arrivedSince = new Set(feedStore.events.map(event => event.id));
  const earlier = restored.filter(event => !arrivedSince.has(event.id));
  feedStore.events = [...feedStore.events, ...earlier].slice(0, CAP);
}

/** Point the feed at `project`, clearing accumulated events when it differs from
 *  the last (a workspace switch must not surface the previous project's changes).
 *  Also lazily arms the singleton subscription on first use. ChangeFeed calls this
 *  from a `project`-keyed effect. */
export function retarget(project: string): void {
  // Arming the singleton subscription is a self-contained side effect with no
  // follow-up here — the store reacts as events land, and startFeedSubscription
  // owns its own error handling — so a lazy call with no await is safe.
  startFeedSubscription();

  if (project === currentProject) {
    return;
  }

  currentProject = project;
  feedStore.events = [];
  adoptLiveWatch(project);
}
