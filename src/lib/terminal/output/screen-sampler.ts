// Context - and a usage-limit stop, with whether a turn is running - is derived
// from agent output, not viewport movement. Sampling after xterm has processed a
// write keeps mouse-wheel/scrollbar renders presentation-only while still reading
// the complete screen that cursor-motion TUIs paint. A fullscreen agent's
// renderer skips cells that are already painted with a cursor-forward, so the
// wire can carry "97% contex" + CSI 1C + " used" and the phrase a parser needs
// never arrives intact; the rendered rows always hold the full text.

import type { Terminal } from "@xterm/xterm";

/** Run `sample` at most once per `intervalMs`: at once when the last sample is
 *  old enough, otherwise once at the end of the interval however many requests
 *  land inside it. */
export function createScreenSampler({ intervalMs, sample }: {
  intervalMs: number;
  sample: () => void;
}) {
  let timer: ReturnType<typeof setTimeout> | undefined;
  let lastSampleAt = 0;
  let isDisposed = false;

  function sampleNow() {
    timer = undefined;

    if (isDisposed) {
      return;
    }

    lastSampleAt = Date.now();
    sample();
  }

  return {
    request() {
      const elapsed = Date.now() - lastSampleAt;
      if (elapsed >= intervalMs) {
        clearTimeout(timer);
        sampleNow();
        return;
      }

      timer ??= setTimeout(sampleNow, intervalMs - elapsed);
    },
    dispose() {
      isDisposed = true;
      clearTimeout(timer);
    }
  };
}

/** The rows the terminal is showing right now, as plain text. */
export function viewportRows(terminal: Terminal) {
  const buffer = terminal.buffer.active;
  const rows: string[] = [];
  for (let row = 0; row < terminal.rows; row += 1) {
    const line = buffer.getLine(buffer.viewportY + row);
    if (line) {
      rows.push(line.translateToString(true));
    }
  }

  return rows;
}
