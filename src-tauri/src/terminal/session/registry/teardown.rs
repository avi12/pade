//! The count of session teardowns still running off the calling thread.

use std::sync::{Arc, Condvar, Mutex, PoisonError};
use std::time::Duration;

/// How many sessions are being ended off the calling thread right now. Ending
/// every session waits for them: the app is about to exit, and a process-tree
/// kill it cut short would leave that agent running with no window to reach it.
#[derive(Debug, Default)]
pub(super) struct Teardowns {
    pub(super) running: Mutex<usize>,
    pub(super) finished: Condvar,
}

impl Teardowns {
    pub(super) fn begin(self: &Arc<Self>) -> Teardown {
        *self.running.lock().unwrap_or_else(PoisonError::into_inner) += 1;
        Teardown(Arc::clone(self))
    }

    /// Block until every teardown has finished, or `timeout` has passed.
    pub(super) fn wait_for_all(&self, timeout: Duration) {
        let running = self.running.lock().unwrap_or_else(PoisonError::into_inner);
        let _ = self
            .finished
            .wait_timeout_while(running, timeout, |running| *running > 0);
    }
}

/// One teardown in flight, counted until it is dropped - after the sessions it
/// ends, or on a panic, so the count can never stay up.
pub(super) struct Teardown(Arc<Teardowns>);

impl Drop for Teardown {
    fn drop(&mut self) {
        let mut running = self
            .0
            .running
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        *running = running.saturating_sub(1);
        self.0.finished.notify_all();
    }
}
