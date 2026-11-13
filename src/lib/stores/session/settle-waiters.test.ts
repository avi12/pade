import { describe, expect, it, vi } from "vitest";

import { createSettleWaiters } from "@/lib/stores/session/settle-waiters";

describe("createSettleWaiters", () => {
  it("settles every waiter on a session once, in the order they were added", () => {
    const waiters = createSettleWaiters();
    const calls: string[] = [];
    waiters.add({ id: "a", settle: () => calls.push("first") });
    waiters.add({ id: "a", settle: () => calls.push("second") });

    waiters.settleAll("a");
    waiters.settleAll("a");

    expect(calls).toEqual(["first", "second"]);
  });

  it("leaves other sessions' waiters alone", () => {
    const waiters = createSettleWaiters();
    const other = vi.fn();
    waiters.add({ id: "b", settle: other });

    waiters.settleAll("a");

    expect(other).not.toHaveBeenCalled();
  });

  it("never settles a waiter that was removed", () => {
    const waiters = createSettleWaiters();
    const removed = vi.fn();
    const kept = vi.fn();
    waiters.add({ id: "a", settle: removed });
    waiters.add({ id: "a", settle: kept });

    waiters.remove({ id: "a", settle: removed });
    waiters.settleAll("a");

    expect(removed).not.toHaveBeenCalled();
    expect(kept).toHaveBeenCalledOnce();
  });

  it("removing the last waiter leaves nothing to settle", () => {
    const waiters = createSettleWaiters();
    const removed = vi.fn();
    waiters.add({ id: "a", settle: removed });

    waiters.remove({ id: "a", settle: removed });
    waiters.settleAll("a");

    expect(removed).not.toHaveBeenCalled();
  });
});
