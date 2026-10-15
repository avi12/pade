import { createSettleWaiters } from "@/lib/stores/settle-waiters";
import { describe, expect, it, vi } from "vitest";

describe("createSettleWaiters", () => {
  it("settles every waiter on a session once, in the order they were added", () => {
    const waiters = createSettleWaiters();
    const calls: string[] = [];
    waiters.add("a", () => calls.push("first"));
    waiters.add("a", () => calls.push("second"));

    waiters.settleAll("a");
    waiters.settleAll("a");

    expect(calls).toEqual(["first", "second"]);
  });

  it("leaves other sessions' waiters alone", () => {
    const waiters = createSettleWaiters();
    const other = vi.fn();
    waiters.add("b", other);

    waiters.settleAll("a");

    expect(other).not.toHaveBeenCalled();
  });

  it("never settles a waiter that was removed", () => {
    const waiters = createSettleWaiters();
    const removed = vi.fn();
    const kept = vi.fn();
    waiters.add("a", removed);
    waiters.add("a", kept);

    waiters.remove("a", removed);
    waiters.settleAll("a");

    expect(removed).not.toHaveBeenCalled();
    expect(kept).toHaveBeenCalledOnce();
  });

  it("removing the last waiter leaves nothing to settle", () => {
    const waiters = createSettleWaiters();
    const removed = vi.fn();
    waiters.add("a", removed);

    waiters.remove("a", removed);
    waiters.settleAll("a");

    expect(removed).not.toHaveBeenCalled();
  });
});
