import { Command, EventType, onMessage, sendMessage } from "@/lib/ipc/messaging";
import type { HandoffTimers } from "@/lib/stores/agent/handoff/handoff-timers";
import { settledValue } from "@/lib/util/async/settled";
import type { UnlistenFn } from "@tauri-apps/api/event";

// Generous: an agent at a near-full context writes its handoff slowly, and a
// timeout that beats the write used to strand the successor without a doc.
const HANDOFF_DOC_TIMEOUT_MS = 300_000;
const HANDOFF_SETTLE_MS = 3_000;
// How often to stat the handoff doc on disk while waiting - the reliable detector
// for a doc the change feed ignores (so the cycle proceeds seconds after the
// write, not on the full timeout).
const HANDOFF_DOC_POLL_MS = 1_000;

/** One direct look at the disk. A failed probe reads as "not there yet", the
 *  answer every caller already falls back on. */
export async function isFileOnDisk(path: string) {
  const probe = await settledValue({
    promise: sendMessage(Command.workspaceProbePath, { path }),
    fallback: null
  });
  return probe?.isFile === true;
}

// Resolve `true` once the doc named `name` exists in the project, `false` when
// the deadline passes without it. Two detectors race: the watcher's change
// event (fast, but SILENT for a doc the feed ignores - a gitignored
// `continue-*.md` never emits, which used to strand the cycle on the full
// timeout), and a direct disk poll (authoritative regardless of ignore rules).
// Whichever sees it first wins.
export function waitForFile({ timers, isStillOpen, sessionId, docPath, name }: {
  timers: HandoffTimers;
  isStillOpen: (sessionId: string) => boolean;
  sessionId: string;
  docPath: string;
  name: string;
}) {
  const { pendingUnlistens, trackTimer, clearTrackedTimer } = timers;
  return new Promise<boolean>(resolve => {
    let unlisten: UnlistenFn | undefined;
    let settleTimer: ReturnType<typeof setTimeout> | undefined;
    let pollTimer: ReturnType<typeof setTimeout> | undefined;
    // Single teardown path: drop the listener + every timer from the pending
    // set, cancel them, then resolve. Used by every exit (match, settle, poll,
    // timeout).
    function finish(isFound: boolean) {
      if (unlisten) {
        pendingUnlistens.delete(unlisten);
        unlisten();
      }

      for (const timer of [deadlineTimer, settleTimer, pollTimer]) {
        clearTrackedTimer(timer);
      }

      resolve(isFound);
    }

    // Read by finish() only at call time (a timer fires well after this line),
    // so a const in the closure is safe.
    const deadlineTimer = trackTimer({
      handler: () => finish(false),
      delayMs: HANDOFF_DOC_TIMEOUT_MS
    });
    const target = name.toLowerCase();

    // Authoritative fallback: stat the doc on disk on a slow tick, so a doc the
    // watcher never reports (ignored path) is still caught within a poll. A
    // session closed meanwhile ends the wait, rather than leaving its status
    // line up for the whole timeout.
    async function pollDisk() {
      if (!isStillOpen(sessionId)) {
        finish(false);
        return;
      }

      if (await isFileOnDisk(docPath)) {
        finish(true);
        return;
      }

      pollTimer = trackTimer({
        handler: pollDisk,
        delayMs: HANDOFF_DOC_POLL_MS
      });
    }

    // Kick off the async watcher subscription from this sync Promise executor.
    // It owns its own error handling, so the deadline timer still resolves the
    // wait even if the subscription never lands.
    async function subscribeToChanges() {
      try {
        unlisten = await onMessage(EventType.feedChange, ({ data: event }) => {
          const isTarget = event.path.replaceAll("\\", "/").toLowerCase().endsWith(target);
          if (!isTarget) {
            return;
          }

          // Restart the short settle window on each matching change; finish only
          // fires once it goes quiet (or the deadline hits first).
          clearTrackedTimer(settleTimer);
          settleTimer = trackTimer({
            handler: () => finish(true),
            delayMs: HANDOFF_SETTLE_MS
          });
        });
        pendingUnlistens.add(unlisten);
      } catch (error) {
        console.warn("pade: handoff doc watch failed to arm, relying on the disk poll", error);
      }
    }

    pollTimer = trackTimer({
      handler: pollDisk,
      delayMs: HANDOFF_DOC_POLL_MS
    });
    subscribeToChanges();
  });
}
