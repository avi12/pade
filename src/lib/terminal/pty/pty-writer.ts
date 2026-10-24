// PTY invokes are asynchronous IPC messages. Keep one write in flight and merge
// input that arrives while it crosses the process boundary: normal keystrokes
// still start immediately, while paste/repeat bursts avoid a costly invoke per
// character and cannot overtake one another.

/** Serializes input across the IPC boundary. The first write in a burst begins
 *  immediately; writes that land while it is in flight share the next send.
 *  Each caller's promise settles after the backend accepted its bytes (or the
 *  endpoint is known gone), so the boot trust-gate and initial-prompt sequence
 *  remains ordered without leaving an awaiter stranded on a failed transport. */
export function createPtyWriter({ send, onTransportLost }: {
  /** Deliver one batch of bytes to the PTY. */
  send: (data: string) => Promise<unknown>;
  /** A send failed: this WebView can no longer reach the session. */
  onTransportLost: () => void;
}) {
  let isWriteInFlight = false;
  let queuedWrites: Array<{
    data: string;
    settle: () => void;
  }> = [];

  async function flush() {
    isWriteInFlight = true;
    while (queuedWrites.length > 0) {
      const writes = queuedWrites;
      queuedWrites = [];
      try {
        await send(writes.map(write => write.data).join(""));
      } catch {
        // No later queued input can reach an unavailable transport. Resolve every
        // waiter and publish the terminal's terminal state instead of retaining
        // invisible promises or retrying an already-disconnected IPC channel.
        onTransportLost();
        const pendingWrites = queuedWrites;
        queuedWrites = [];
        for (const write of pendingWrites) {
          write.settle();
        }
      } finally {
        for (const write of writes) {
          write.settle();
        }
      }
    }
    isWriteInFlight = false;
  }

  return function write(data: string) {
    return new Promise<void>(settle => {
      queuedWrites.push({
        data,
        settle
      });

      if (!isWriteInFlight) {
        flush();
      }
    });
  };
}
