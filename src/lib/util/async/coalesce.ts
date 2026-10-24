// One run at a time for an async refresh, without losing a request: a call
// that lands while a run is in flight queues exactly one more pass, so the
// last request is always answered by a run that started after it.

export function coalesced(task: () => Promise<void>) {
  let isRunning = false;
  let isRequestedDuringRun = false;

  return async function run() {
    if (isRunning) {
      isRequestedDuringRun = true;
      return;
    }

    isRunning = true;
    try {
      do {
        isRequestedDuringRun = false;
        await task();
      } while (isRequestedDuringRun);
    } finally {
      isRunning = false;
    }
  };
}
