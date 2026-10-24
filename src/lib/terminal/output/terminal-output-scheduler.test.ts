import { createTerminalOutputScheduler } from "@/lib/terminal/output/terminal-output-scheduler";
import {
  afterEach,
  beforeEach,
  describe,
  expect,
  it,
  vi
} from "vitest";

const FRAME_MS = 16;

function schedulerAt(context: {
  isShown: boolean;
  isWindowFocused: boolean;
  isReadingScrollback: boolean;
}) {
  const written: string[] = [];
  const scheduler = createTerminalOutputScheduler({
    write: output => written.push(output),
    flushContext: () => context
  });
  return {
    scheduler,
    written,
    context
  };
}

const FRONT = {
  isShown: true,
  isWindowFocused: true,
  isReadingScrollback: false
};

describe("createTerminalOutputScheduler", () => {
  beforeEach(() => {
    vi.useFakeTimers();
    vi.stubGlobal("requestAnimationFrame", (callback: () => void) => setTimeout(callback, FRAME_MS));
    vi.stubGlobal("cancelAnimationFrame", (handle: ReturnType<typeof setTimeout>) => clearTimeout(handle));
  });

  afterEach(() => {
    vi.unstubAllGlobals();
    vi.useRealTimers();
  });

  it("joins a front pane's chunks into one write per frame", () => {
    const { scheduler, written } = schedulerAt({ ...FRONT });
    scheduler.queue("a");
    scheduler.queue("b");
    expect(written).toEqual([]);

    vi.advanceTimersByTime(FRAME_MS);
    expect(written).toEqual(["ab"]);
  });

  it("writes a background pane at the bounded cadence", () => {
    const { scheduler, written } = schedulerAt({
      ...FRONT,
      isShown: false
    });
    scheduler.queue("a");
    vi.advanceTimersByTime(FRAME_MS);
    expect(written).toEqual([]);

    vi.advanceTimersByTime(250);
    expect(written).toEqual(["a"]);
  });

  it("holds output while the document scrolls and writes it once the wheel goes quiet", () => {
    const { scheduler, written } = schedulerAt({ ...FRONT });
    scheduler.noteScroll();
    scheduler.queue("a");
    vi.advanceTimersByTime(100);
    expect(written).toEqual([]);

    vi.advanceTimersByTime(20);
    expect(written).toEqual(["a"]);
  });

  it("never holds output past the deadline of a long scroll", () => {
    const { scheduler, written } = schedulerAt({ ...FRONT });
    scheduler.queue("a");
    for (let elapsed = 0; elapsed < 1000; elapsed += 100) {
      scheduler.noteScroll();
      vi.advanceTimersByTime(100);
    }

    expect(written).toEqual(["a"]);
  });

  it("writes nothing once disposed", () => {
    const { scheduler, written } = schedulerAt({ ...FRONT });
    scheduler.queue("a");
    scheduler.dispose();
    vi.advanceTimersByTime(1000);
    expect(written).toEqual([]);
  });
});
