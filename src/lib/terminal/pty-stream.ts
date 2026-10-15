// Until the session's history has been replayed the live chunks are only parked,
// not written: the PTY may already be running and the terminal is empty, so
// writing them now would paint the tail of a conversation whose beginning is
// missing. Each chunk carries its position in the stream, so once the history is
// in, the ones it already contains can be dropped and the rest written in order.

import type { PtyChunk } from "@/lib/types";

export function createPtyStream(consume: (chunk: PtyChunk) => void) {
  const pendingChunks: PtyChunk[] = [];
  let isReplayed = false;

  return {
    /** A live chunk of this session's output. */
    accept(chunk: PtyChunk) {
      if (!isReplayed) {
        pendingChunks.push(chunk);
        return;
      }

      consume(chunk);
    },
    /** The history is in, authoritative up to `seq`: anything at or below it is
     *  already painted, so only the newer parked chunks still go in - in order,
     *  and before any further live chunk. */
    goLive(seq: number) {
      for (const chunk of pendingChunks) {
        if (chunk.seq > seq) {
          consume(chunk);
        }
      }

      pendingChunks.length = 0;
      isReplayed = true;
    }
  };
}
