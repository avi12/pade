// Persistent accumulation of the Change Feed's events (SoC: cross-component state
// lives in lib/stores). ChangeFeed mounts and unmounts as the side panel switches,
// so keeping the feed in component-local state emptied it on every switch — the
// backend keeps no replay. This module owns the single live subscription to the
// feed stream, started once and never torn down, so events keep arriving while the
// panel is closed and survive its remount.

import { feed } from "@/lib/bridge";
import { baseName } from "@/lib/paths";
import type { ChangeEvent } from "@/lib/types";

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

/** Subscribe once to the backend feed stream. Idempotent — a second call is a
 *  no-op — and never unsubscribed, so events accumulate across panel remounts. */
async function startFeedSubscription(): Promise<void> {
  if (subscribed) {
    return;
  }

  subscribed = true;
  await feed.onChange(event => {
    const isScratchFile = TEMP_FILE.test(baseName(event.path));
    if (isScratchFile) {
      return;
    }

    feedStore.events = [event, ...feedStore.events].slice(0, CAP);
  });
}

/** Point the feed at `project`, clearing accumulated events when it differs from
 *  the last (a workspace switch must not surface the previous project's changes).
 *  Also lazily arms the singleton subscription on first use. ChangeFeed calls this
 *  from a `project`-keyed effect. */
export function retarget(project: string): void {
  // Registering the listener is a self-contained side effect with no follow-up
  // here — the store reacts as events land — so a lazy fire-and-forget is safe.
  void startFeedSubscription();

  if (project === currentProject) {
    return;
  }

  currentProject = project;
  feedStore.events = [];
}
