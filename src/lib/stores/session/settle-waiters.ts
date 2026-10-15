// Per-session promise resolvers waiting for one event - "the session went
// idle", "the agent stopped reporting work". Event-driven, no polling: the
// store that sees the event settles every waiter on that session at once.
// Resolver bookkeeping only, so a plain Map: nothing renders from it.

type Settle = () => void;

export function createSettleWaiters() {
  const waitersById = new Map<string, Settle[]>();

  /** Wait on `id`'s next settle. */
  function add(id: string, settle: Settle) {
    waitersById.set(id, [...(waitersById.get(id) ?? []), settle]);
  }

  /** Stop waiting - a waiter that gave up (a timeout) without being settled. */
  function remove(id: string, settle: Settle) {
    const remaining = (waitersById.get(id) ?? []).filter(waiter => waiter !== settle);
    if (remaining.length > 0) {
      waitersById.set(id, remaining);
    } else {
      waitersById.delete(id);
    }
  }

  /** Settle, and forget, every waiter on `id`. */
  function settleAll(id: string) {
    const waiters = waitersById.get(id);
    if (!waiters) {
      return;
    }

    waitersById.delete(id);
    for (const settle of waiters) {
      settle();
    }
  }

  return {
    add,
    remove,
    settleAll
  };
}
