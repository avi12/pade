import { IDLE_MS } from "@/lib/terminal/activity-timing";
import { createSessionActivity } from "@/lib/terminal/sessionActivity.svelte";
import { ENTER } from "@/lib/terminal/terminal-sequences";
import { SessionStatus } from "@/lib/types";
import {
  afterEach,
  beforeEach,
  describe,
  expect,
  it,
  vi
} from "vitest";

function settledActivity() {
  const onSettled = vi.fn();
  const activity = createSessionActivity({ onSettled });
  activity.markActivity();
  vi.advanceTimersByTime(IDLE_MS);
  return {
    activity,
    onSettled
  };
}

describe("createSessionActivity", () => {
  beforeEach(() => {
    vi.useFakeTimers();
    vi.setSystemTime(100_000);
  });

  afterEach(() => {
    vi.useRealTimers();
  });

  it("works while output flows and settles to ready once it falls quiet", () => {
    const { activity, onSettled } = settledActivity();
    expect(activity.status).toBe(SessionStatus.enum.ready);
    expect(onSettled).toHaveBeenCalledTimes(1);
  });

  it("does not count a keystroke's echo as work, but does count a submit's", () => {
    const { activity } = settledActivity();
    vi.advanceTimersByTime(1000);
    activity.noteInput("x");
    activity.markActivity();
    expect(activity.status).toBe(SessionStatus.enum.ready);

    vi.advanceTimersByTime(1000);
    activity.noteInput(ENTER);
    activity.markActivity();
    expect(activity.status).toBe(SessionStatus.enum.working);
  });

  it("does not count a resize repaint as work", () => {
    const { activity } = settledActivity();
    vi.advanceTimersByTime(1000);
    activity.noteResize();
    activity.markActivity();
    expect(activity.status).toBe(SessionStatus.enum.ready);
  });

  it("stays exited and seeds an attached session as ready", () => {
    const attached = createSessionActivity({ onSettled() {} });
    attached.markAttached();
    expect(attached.status).toBe(SessionStatus.enum.ready);

    attached.markExited();
    attached.markActivity();
    expect(attached.status).toBe(SessionStatus.enum.exited);
  });
});
