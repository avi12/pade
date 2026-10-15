// The bookkeeping every automatic-recovery loop shares - usage-limit auto-resume
// (lib/stores/usageResume) and API-error auto-retry (lib/stores/apiErrorRetry):
// one pending timer per session, the status note the app shell shows while any is
// pending, the scan that starts a freshly flagged session and forgets one whose
// tab is gone, and teardown. Each loop keeps its own detection state (a `hits` map
// the output sniffers fill) and decides what one tick of a session does.

import { measuredContextPercentage } from "@/lib/stores/context.svelte";
import type { AgentSession } from "@/lib/types";
import { SvelteMap, SvelteSet } from "svelte/reactivity";

/** What the app shell provides to every recovery loop. */
export interface RecoveryHost {
  sessions: () => AgentSession[];
  /** Whether the user opted out via prefs.autoResume. */
  isOptedOut: () => boolean;
  /** The percent-of-context past which recovery hands off instead - the resolved
   *  prefs.handoffPct (`effective.handoffPercentage`). */
  thresholdPercentage: () => number;
  /** Hand a session off to a fresh agent now (the auto-handoff flow) - for a
   *  session whose context window is too full to keep going. */
  forceHandoff: (session: AgentSession) => void;
}

/** A sniffed session awaiting recovery; `isScheduled` once its loop has started. */
export interface RecoveryHit {
  isScheduled: boolean;
}

/** One loop's timers, note and scan over its `hits`. `tick` runs a session's next
 *  step when its timer fires; `firstDelay` is how long after being flagged the
 *  first one runs; `pendingNote`, when given, is the note shown whenever a step is
 *  scheduled (a loop without one sets `note` itself); `forgetState` clears whatever
 *  else the loop keeps per session. */
export function createRecoveryLoop<Hit extends RecoveryHit>({
  host,
  hits,
  firstDelay,
  tick,
  pendingNote,
  forgetState
}: {
  host: RecoveryHost;
  hits: SvelteMap<string, Hit>;
  firstDelay: number;
  tick: (session: AgentSession) => Promise<void>;
  pendingNote?: (session: AgentSession) => string;
  forgetState?: (id: string) => void;
}) {
  let note = $state("");
  let isDisposed = false;
  const timers = new SvelteMap<string, ReturnType<typeof setTimeout>>();

  function clearTimer(id: string) {
    const timer = timers.get(id);
    if (timer === undefined) {
      return;
    }

    clearTimeout(timer);
    timers.delete(id);
  }

  // End one session's recovery: drop its timer, hit and loop state, and clear the
  // shared note once nothing else is pending.
  function forget(id: string) {
    clearTimer(id);
    hits.delete(id);
    forgetState?.(id);

    if (timers.size === 0) {
      note = "";
    }
  }

  function schedule({ session, delay }: {
    session: AgentSession;
    delay: number;
  }) {
    if (isDisposed) {
      return;
    }

    if (pendingNote) {
      note = pendingNote(session);
    }

    timers.set(
      session.id, setTimeout(async () => {
        timers.delete(session.id);
        await tick(session);
      }, delay)
    );
  }

  // Start the loop of each freshly flagged session; forget sessions whose tab is gone.
  function check() {
    if (host.isOptedOut()) {
      return;
    }

    const alive = new SvelteSet(host.sessions().map(session => session.id));
    for (const id of [...hits.keys(), ...timers.keys()]) {
      if (!alive.has(id)) {
        forget(id);
      }
    }

    for (const session of host.sessions()) {
      const hit = hits.get(session.id);
      if (!hit || hit.isScheduled) {
        continue;
      }

      hits.set(session.id, {
        ...hit,
        isScheduled: true
      });
      schedule({
        session,
        delay: firstDelay
      });
    }
  }

  function dispose() {
    isDisposed = true;
    for (const timer of timers.values()) {
      clearTimeout(timer);
    }

    timers.clear();
  }

  return {
    /** Status line shown while recovery is pending ("" when idle). */
    get note() {
      return note;
    },
    set note(text: string) {
      note = text;
    },
    /** Whether the loop was torn down - an in-flight tick must not reschedule. */
    get isDisposed() {
      return isDisposed;
    },
    /** Whether the session is still open in the app shell. */
    isOpen: (id: string) => host.sessions().some(session => session.id === id),
    /** Whether the session's context window has room to keep going - true when
     *  the agent has vouched for no fill yet. Below the handoff threshold, a
     *  recovery continues the session; at or past it, the session is handed off. */
    hasContextRoom(id: string) {
      const percentage = measuredContextPercentage(id);
      return percentage === null || percentage < host.thresholdPercentage();
    },
    forget,
    schedule,
    check,
    dispose
  };
}
