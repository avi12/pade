import { SUBMIT_VERIFY_MS } from "@/lib/terminal/activity-timing";
import { createSessionActivity } from "@/lib/terminal/sessionActivity.svelte";
import { SessionStatus } from "@/lib/types";
import {
  afterEach,
  beforeEach,
  describe,
  expect,
  it,
  vi
} from "vitest";

const ENTER_ECHO_DELAY_MS = 20;

describe("SUBMIT_VERIFY_MS", () => {
  beforeEach(() => {
    vi.useFakeTimers();
  });

  afterEach(() => {
    vi.useRealTimers();
  });

  it("reads an Enter the composer swallowed as unsent, though its repaint arrives after the Enter", () => {
    const activity = createSessionActivity({ onSettled: vi.fn() });
    activity.markActivity();
    vi.advanceTimersByTime(ENTER_ECHO_DELAY_MS);
    activity.markActivity();

    vi.advanceTimersByTime(SUBMIT_VERIFY_MS - ENTER_ECHO_DELAY_MS);

    expect(activity.status).toBe(SessionStatus.enum.ready);
  });

  it("reads an agent still printing as having taken the prompt up", () => {
    const activity = createSessionActivity({ onSettled: vi.fn() });
    activity.markActivity();
    vi.advanceTimersByTime(SUBMIT_VERIFY_MS - ENTER_ECHO_DELAY_MS);
    activity.markActivity();
    vi.advanceTimersByTime(ENTER_ECHO_DELAY_MS);

    expect(activity.status).toBe(SessionStatus.enum.working);
  });
});
