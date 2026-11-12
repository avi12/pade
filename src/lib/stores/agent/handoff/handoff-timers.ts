import type { UnlistenFn } from "@tauri-apps/api/event";
import { SvelteSet } from "svelte/reactivity";

/** Every timer and feed listener one handoff controller has in flight. A handoff
 *  can pend for minutes, so these must be torn down if the shell unmounts first -
 *  otherwise the watcher subscription and timers leak. Tracked here so `dispose()`
 *  can clear every still-pending wait from one place. */
export function createHandoffTimers() {
  const pendingUnlistens = new SvelteSet<UnlistenFn>();
  const pendingTimers = new SvelteSet<ReturnType<typeof setTimeout>>();

  // Track one timer in the pending set and return its id, so every timer we
  // create is registered for teardown in exactly one place.
  function trackTimer({ handler, delayMs }: {
    handler: () => void;
    delayMs: number;
  }) {
    const timer = setTimeout(handler, delayMs);
    pendingTimers.add(timer);
    return timer;
  }

  function clearTrackedTimer(timer: ReturnType<typeof setTimeout> | undefined) {
    if (timer === undefined) {
      return;
    }

    pendingTimers.delete(timer);
    clearTimeout(timer);
  }

  // Resolve after the delay, tracking the timer so dispose() tears it down.
  function afterDelay(delayMs: number) {
    return new Promise<void>(resolve => {
      trackTimer({
        handler: () => resolve(),
        delayMs
      });
    });
  }

  // Tear down every still-pending wait (listener + timers).
  function dispose() {
    for (const unlisten of pendingUnlistens) {
      unlisten();
    }

    for (const timer of pendingTimers) {
      clearTimeout(timer);
    }

    pendingUnlistens.clear();
    pendingTimers.clear();
  }

  return {
    pendingUnlistens,
    trackTimer,
    clearTrackedTimer,
    afterDelay,
    dispose
  };
}

export type HandoffTimers = ReturnType<typeof createHandoffTimers>;
