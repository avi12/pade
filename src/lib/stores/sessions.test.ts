import {
  dropSessionStatus,
  isSessionIdle,
  sessionStatus,
  setSessionStatus,
  whenSessionIdle
} from "@/lib/stores/sessions.svelte";
import { SessionStatus } from "@/lib/types";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

// Each promise resolution needs a microtask turn; fake timers cover the
// timeout path without real waiting.
async function settled(promise: Promise<void>): Promise<boolean> {
  let resolved = false;
  void promise.then(() => (resolved = true));
  await Promise.resolve();
  await Promise.resolve();
  return resolved;
}

describe("session status", () => {
  afterEach(() => dropSessionStatus("s1"));

  it("defaults an unknown session to starting", () => {
    expect(sessionStatus("s1")).toBe(SessionStatus.enum.starting);
  });

  it("reads ready and exited as idle, working and starting as busy", () => {
    expect(isSessionIdle("s1")).toBe(false);

    setSessionStatus({
      id: "s1",
      status: SessionStatus.enum.working
    });
    expect(isSessionIdle("s1")).toBe(false);

    setSessionStatus({
      id: "s1",
      status: SessionStatus.enum.ready
    });
    expect(isSessionIdle("s1")).toBe(true);

    setSessionStatus({
      id: "s1",
      status: SessionStatus.enum.exited
    });
    expect(isSessionIdle("s1")).toBe(true);
  });
});

describe("whenSessionIdle", () => {
  beforeEach(() => vi.useFakeTimers());
  afterEach(() => {
    dropSessionStatus("s1");
    vi.useRealTimers();
  });

  it("resolves immediately for an already-idle session", async () => {
    setSessionStatus({
      id: "s1",
      status: SessionStatus.enum.ready
    });
    expect(await settled(whenSessionIdle({
      id: "s1",
      timeoutMs: 1_000
    }))).toBe(true);
  });

  it("waits while the session works and resolves when it turns ready", async () => {
    setSessionStatus({
      id: "s1",
      status: SessionStatus.enum.working
    });
    const wait = whenSessionIdle({
      id: "s1",
      timeoutMs: 60_000
    });
    expect(await settled(wait)).toBe(false);

    setSessionStatus({
      id: "s1",
      status: SessionStatus.enum.ready
    });
    expect(await settled(wait)).toBe(true);
  });

  it("resolves when the working session exits instead", async () => {
    setSessionStatus({
      id: "s1",
      status: SessionStatus.enum.working
    });
    const wait = whenSessionIdle({
      id: "s1",
      timeoutMs: 60_000
    });

    setSessionStatus({
      id: "s1",
      status: SessionStatus.enum.exited
    });
    expect(await settled(wait)).toBe(true);
  });

  it("resolves when the session is dropped mid-wait", async () => {
    setSessionStatus({
      id: "s1",
      status: SessionStatus.enum.working
    });
    const wait = whenSessionIdle({
      id: "s1",
      timeoutMs: 60_000
    });

    dropSessionStatus("s1");
    expect(await settled(wait)).toBe(true);
  });

  it("gives up at the timeout so a wedged agent cannot trap the leave", async () => {
    setSessionStatus({
      id: "s1",
      status: SessionStatus.enum.working
    });
    const wait = whenSessionIdle({
      id: "s1",
      timeoutMs: 1_000
    });
    expect(await settled(wait)).toBe(false);

    vi.advanceTimersByTime(1_000);
    expect(await settled(wait)).toBe(true);
  });

  it("settles every waiter on one session, once each", async () => {
    setSessionStatus({
      id: "s1",
      status: SessionStatus.enum.working
    });
    const first = whenSessionIdle({
      id: "s1",
      timeoutMs: 60_000
    });
    const second = whenSessionIdle({
      id: "s1",
      timeoutMs: 60_000
    });

    setSessionStatus({
      id: "s1",
      status: SessionStatus.enum.ready
    });
    expect(await settled(first)).toBe(true);
    expect(await settled(second)).toBe(true);
  });
});
