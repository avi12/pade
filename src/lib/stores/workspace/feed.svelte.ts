// Persistent accumulation of the Change Feed's events (SoC: cross-component state
// lives in lib/stores). ChangeFeed mounts and unmounts as the side panel switches,
// so keeping the feed in component-local state emptied it on every switch - the
// backend keeps no replay. This module owns the single live subscription to the
// feed stream, started once and never torn down, so events keep arriving while the
// panel is closed and survive its remount.

import { eventsForLiveWatch, readFeedSnapshot, saveFeedSnapshot } from "@/lib/feed/watch/feed-snapshot";
import { Command, EventType, onMessage, sendMessage } from "@/lib/ipc/messaging";
import type { ChangeEvent, WatchStatus } from "@/lib/types";
import { baseName, isPathInside } from "@/lib/util/paths";
import { SvelteMap } from "svelte/reactivity";

// Newest first. Capped so a busy agent session can't grow this unbounded.
const MAXIMUM_FEED_EVENTS = 300;

// Editor/tool scratch files that churn during an atomic save (write to a temp
// name, then rename over the target) - noise, not real changes. Match the shapes
// the feed sees: a `.tmp.` infix, a `_tmp_` scratch name, a vim swap, a trailing
// `~` backup, or a long numeric atomic-save suffix.
const TEMP_FILE_RE = /^_tmp_|\.tmp\.|\.sw[a-z]$|~$|\.\d{7,}$/i;

/** The live feed - newest-first and capped. Reactive, so the panel re-renders as
 *  events land whether or not it is currently mounted. */
export const feedStore = $state<{ events: ChangeEvent[] }>({ events: [] });

// The project the shown events belong to - the current session's.
let currentProject: string | null = null;

// Every other project's feed, put aside while another session's project is in
// front and shown again when one of its sessions is. The backend watches only
// the front project, so a project behind gathers nothing meanwhile.
const feedsBehind = new SvelteMap<string, ChangeEvent[]>();

// The stream is subscribed exactly once for the process lifetime.
let isSubscribed = false;

// The backend watch the accumulated events came from. A reload saves the feed
// under it and restores only into that same watch (see feed-snapshot). Learned on
// retarget, and again on the first event when that was too early: a freshly
// opened window's panel retargets before `watch_start` has armed the watch, so
// the first answer carries no root - and with no identity to save under, the
// unload save skipped and an F5 emptied the feed.
let liveWatch: WatchStatus | null = null;

async function learnLiveWatch() {
  try {
    liveWatch = await sendMessage(Command.watchStatus);
  } catch {
    liveWatch = null;
  }
}

function knowsLiveWatch() {
  return liveWatch !== null && liveWatch.armedAt !== null && liveWatch.root === currentProject;
}

// Hand the feed to the reloaded page. `pagehide` is the last synchronous moment
// before the webview drops this page, so the save happens once per unload rather
// than re-serializing up to MAXIMUM_FEED_EVENTS events on every change.
function saveFeedOnUnload() {
  if (!liveWatch?.root || liveWatch.armedAt === null) {
    return;
  }

  saveFeedSnapshot({
    root: liveWatch.root,
    armedAt: liveWatch.armedAt,
    events: feedStore.events
  });
}

// A real change in the project shown - not editor scratch, and not a late
// change from the watch being re-rooted away, which is the project behind's.
function isShownFeedChange(event: ChangeEvent) {
  const isScratchFile = TEMP_FILE_RE.test(baseName(event.path));
  const isInShownProject = currentProject !== null && isPathInside({
    path: event.path,
    root: currentProject
  });
  return !isScratchFile && isInShownProject;
}

/** Subscribe once to the backend feed stream. Idempotent - a second call is a
 *  no-op - and never unsubscribed, so events accumulate across panel remounts. */
async function startFeedSubscription() {
  if (isSubscribed) {
    return;
  }

  isSubscribed = true;
  addEventListener("pagehide", saveFeedOnUnload);
  try {
    await onMessage(EventType.feedChange, ({ data: event }) => {
      if (!isShownFeedChange(event)) {
        return;
      }

      feedStore.events = [event, ...feedStore.events].slice(0, MAXIMUM_FEED_EVENTS);

      // An event proves the watch is armed, so an identity missed at retarget
      // can be learned now.
      if (!knowsLiveWatch()) {
        learnLiveWatch();
      }
    });
    // The ignore rules are live: editing (or creating, or deleting) a .gitignore
    // - and a mid-session `git init` - re-filters what the feed already shows, so
    // a path the project just started ignoring drops out instead of lingering.
    // The backend is the one authority on "ignored" (git's own rules in a repo,
    // the root .gitignore + tech inference otherwise); the store only asks.
    await onMessage(EventType.feedIgnoreChanged, async () => {
      const paths = [...new Set(feedStore.events.map(event => event.path))];
      if (paths.length === 0) {
        return;
      }

      const nowIgnored = new Set(await sendMessage(Command.feedIgnored, { paths }));
      if (nowIgnored.size > 0) {
        feedStore.events = feedStore.events.filter(event => !nowIgnored.has(event.path));
      }
    });
  } catch {
    // Re-arm on a failed subscribe so a later retarget can try again.
    isSubscribed = false;
    removeEventListener("pagehide", saveFeedOnUnload);
  }
}

// Learn the watch now feeding `project`, and - after a reload - put back the
// events that same watch had already surfaced, behind any that arrived since.
async function adoptLiveWatch(project: string) {
  await learnLiveWatch();

  if (!liveWatch) {
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
  feedStore.events = [...feedStore.events, ...earlier].slice(0, MAXIMUM_FEED_EVENTS);
}

/** Point the feed at `project`: the previous project's events are put aside and
 *  `project`'s own come back, so switching sessions shows each one's changes.
 *  Also lazily arms the singleton subscription on first use. The window's
 *  services call this whenever the front project changes, panel open or not. */
export function retargetFeed(project: string) {
  // Arming the singleton subscription is a self-contained side effect with no
  // follow-up here - the store reacts as events land, and startFeedSubscription
  // owns its own error handling - so a lazy call with no await is safe.
  startFeedSubscription();

  if (project === currentProject) {
    return;
  }

  if (currentProject !== null) {
    feedsBehind.set(currentProject, feedStore.events);
  }

  currentProject = project;
  feedStore.events = feedsBehind.get(project) ?? [];
  feedsBehind.delete(project);
  adoptLiveWatch(project);
}
