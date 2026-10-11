// One run at a time for an async refresh, without losing a request: a call
// that lands while a run is in flight queues exactly one more pass, so the
// last request is always answered by a run that started after it.

/** `task` wrapped so overlapping calls coalesce into at most one extra run. */
export function coalesced(task: () => Promise<void>): () => Promise<void> {
  let running = false;
  let requestedDuringRun = false;

  return async function run() {
    if (running) {
      requestedDuringRun = true;
      return;
    }

    running = true;
    try {
      do {
        requestedDuringRun = false;
        await task();
      } while (requestedDuringRun);
    } finally {
      running = false;
    }
  };
}
