// Persistent accumulation of the Change Feed's events (SoC: cross-component state
// lives in lib/stores). ChangeFeed mounts and unmounts as the side panel switches,
// so keeping the feed in component-local state emptied it on every switch - the
// backend keeps no replay. This module owns the single live subscription to the
// feed stream, started once and never torn down, so events keep arriving while the
// panel is closed and survive its remount.
//
// The window watches every project it holds a session of, and each event names
// the held root it belongs to, so every project gathers its own feed whether it
// is in front or not; the panel shows the front project's.

import { eventsForLiveWatch, readFeedSnapshot, saveFeedSnapshot } from "@/lib/feed/watch/feed-snapshot";
import { Command, EventType, onMessage, sendMessage } from "@/lib/ipc/messaging";
import { frontProject } from "@/lib/stores/workspace/frontProject.svelte";
import type { ChangeEvent } from "@/lib/types";
import { baseName } from "@/lib/util/paths";
import { SvelteMap } from "svelte/reactivity";

// Newest first, per project. Capped so a busy agent session can't grow this
// unbounded.
const MAXIMUM_FEED_EVENTS = 300;

// Editor/tool scratch files that churn during an atomic save (write to a temp
// name, then rename over the target) - noise, not real changes. Match the shapes
// the feed sees: a `.tmp.` infix, a `_tmp_` scratch name, a vim swap, a trailing
// `~` backup, or a long numeric atomic-save suffix.
const TEMP_FILE_RE = /^_tmp_|\.tmp\.|\.sw[a-z]$|~$|\.\d{7,}$/i;

const NO_EVENTS: ChangeEvent[] = [];

// Each held project's feed, keyed by its root as the window holds it - the
// spelling `watch_projects` was sent and every event's `root` carries back.
const feedsByProject = new SvelteMap<string, ChangeEvent[]>();

// When each held project's backend watch armed. A reload saves a project's feed
// under it and restores only into that same watch (see feed-snapshot). Learned
// when the project is first held, and again on its first event when that was too
// early: a freshly held project is asked about before `watch_projects` has armed
// it, so the first answer carries no arming - and with no identity to save
// under, the unload save skipped it and an F5 emptied its feed. Bookkeeping,
// never rendered.
// eslint-disable-next-line svelte/prefer-svelte-reactivity -- never rendered
const armedAtByProject = new Map<string, number>();

// The stream is subscribed exactly once for the process lifetime.
let isSubscribed = false;

/** The front project's live feed - newest-first and capped. Reactive, so the
 *  panel re-renders as events land whether or not it is currently mounted, and
 *  shows another project's the moment that one comes to the front. */
export const feedStore = {
  get events() {
    return feedsByProject.get(frontProject()) ?? NO_EVENTS;
  }
};

// When the backend's watch of `root` armed; `null` while it is not armed yet,
// or when the backend cannot say - either way there is no watch to tie to.
async function liveArmedAt(root: string) {
  try {
    const status = await sendMessage(Command.watchStatus, { root });
    return status.armedAt;
  } catch {
    return null;
  }
}

// Learn the watch now feeding `root`, and - after a reload - put back the events
// that same watch had already surfaced, behind any that arrived since.
async function adoptLiveWatch(root: string) {
  const armedAt = await liveArmedAt(root);
  const isStillHeld = feedsByProject.has(root);
  if (armedAt === null || !isStillHeld) {
    return;
  }

  armedAtByProject.set(root, armedAt);
  const restored = eventsForLiveWatch({
    snapshot: readFeedSnapshot(),
    root,
    watch: {
      armedAt
    }
  });
  if (restored.length === 0) {
    return;
  }

  const current = feedsByProject.get(root) ?? NO_EVENTS;
  const arrivedSince = new Set(current.map(event => event.id));
  const earlier = restored.filter(event => !arrivedSince.has(event.id));
  feedsByProject.set(root, [...current, ...earlier].slice(0, MAXIMUM_FEED_EVENTS));
}

// Hand every held project's feed to the reloaded page. `pagehide` is the last
// synchronous moment before the webview drops this page, so the save happens once
// per unload rather than re-serializing every feed on every change.
function saveFeedsOnUnload() {
  const feeds = [];
  for (const [root, events] of feedsByProject) {
    const armedAt = armedAtByProject.get(root);
    if (armedAt !== undefined) {
      feeds.push({
        root,
        armedAt,
        events
      });
    }
  }

  saveFeedSnapshot({ feeds });
}

function fileChange(event: ChangeEvent) {
  const events = feedsByProject.get(event.root);
  const isScratchFile = TEMP_FILE_RE.test(baseName(event.path));
  if (events === undefined || isScratchFile) {
    return;
  }

  feedsByProject.set(event.root, [event, ...events].slice(0, MAXIMUM_FEED_EVENTS));

  // An event proves the watch is armed, so an identity missed when the project
  // was first held can be learned now.
  if (!armedAtByProject.has(event.root)) {
    adoptLiveWatch(event.root);
  }
}

// The ignore rules are live: editing (or creating, or deleting) a .gitignore - and
// a mid-session `git init` - re-filters what that project's feed already shows,
// so a path the project just started ignoring drops out instead of lingering.
// The backend is the one authority on "ignored" (git's own rules in a repo, the
// root .gitignore + tech inference otherwise); the store only asks.
async function refilterIgnored(root: string) {
  const events = feedsByProject.get(root) ?? NO_EVENTS;
  const paths = [...new Set(events.map(event => event.path))];
  if (paths.length === 0) {
    return;
  }

  const nowIgnored = new Set(await sendMessage(Command.feedIgnored, { paths }));
  const current = feedsByProject.get(root);
  if (nowIgnored.size === 0 || current === undefined) {
    return;
  }

  feedsByProject.set(root, current.filter(event => !nowIgnored.has(event.path)));
}

/** Subscribe once to the backend feed stream. Idempotent - a second call is a
 *  no-op - and never unsubscribed, so events accumulate across panel remounts. */
async function startFeedSubscription() {
  if (isSubscribed) {
    return;
  }

  isSubscribed = true;
  addEventListener("pagehide", saveFeedsOnUnload);
  try {
    await onMessage(EventType.feedChange, ({ data: event }) => fileChange(event));
    await onMessage(EventType.feedIgnoreChanged, async ({ data: { root } }) => await refilterIgnored(root));
  } catch {
    // Re-arm on a failed subscribe so the next held-set change can try again.
    isSubscribed = false;
    removeEventListener("pagehide", saveFeedsOnUnload);
  }
}

/** Gather a feed for each of `projects` - the projects the window holds a
 *  session of - and drop the feed of any project no longer among them: the
 *  window holds no session of it any more, so it would never be shown again.
 *  A project newly held learns its live watch, restoring what a reload would
 *  have lost. Also lazily arms the singleton subscription on first use. */
export function keepFeedsOf(projects: string[]) {
  // Arming the singleton subscription is a self-contained side effect with no
  // follow-up here - the store reacts as events land, and startFeedSubscription
  // owns its own error handling - so a lazy call with no await is safe.
  startFeedSubscription();

  for (const project of feedsByProject.keys()) {
    if (!projects.includes(project)) {
      feedsByProject.delete(project);
      armedAtByProject.delete(project);
    }
  }

  for (const project of projects) {
    if (!feedsByProject.has(project)) {
      feedsByProject.set(project, []);
      adoptLiveWatch(project);
    }
  }
}
