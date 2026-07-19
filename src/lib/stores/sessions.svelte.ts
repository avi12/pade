// Shared session-status store (SoC: cross-component state lives in lib/stores).
// The Terminal owns the truth about a PTY's liveness (idle detection etc.) and
// publishes it here; the top-bar tabs read it to show a per-session status dot,
// and a deliberate leave awaits `whenSessionIdle` so nothing mid-flight is
// severed. A SvelteMap keeps reads reactive across components.

import { SessionStatus } from "@/lib/types";
import { SvelteMap } from "svelte/reactivity";

const statuses = new SvelteMap<string, SessionStatus>();

// Pending `whenSessionIdle` resolvers, settled the moment the session's status
// turns idle (or the session is dropped). Event-driven — no polling. A
// SvelteMap only for consistency in a rune module; nothing renders from it.
const idleWaiters = new SvelteMap<string, (() => void)[]>();

/** Publish the status of a session (called by its Terminal). */
export function setSessionStatus({ id, status }: {
  id: string;
  status: SessionStatus;
}): void {
  statuses.set(id, status);

  if (isIdleStatus(status)) {
    settleIdleWaiters(id);
  }
}

/** Read a session's status (reactive) — defaults to `starting` until known. */
export function sessionStatus(id: string): SessionStatus {
  return statuses.get(id) ?? SessionStatus.enum.starting;
}

/** Forget a session's status when its tab closes. A gone session has nothing
 *  left to sever, so any idle waiter on it settles too. */
export function dropSessionStatus(id: string): void {
  statuses.delete(id);
  settleIdleWaiters(id);
}

function isIdleStatus(status: SessionStatus): boolean {
  return status === SessionStatus.enum.ready || status === SessionStatus.enum.exited;
}

/** Is the session at an idle prompt (or already gone)? `ready` is the real idle
 *  signal — the Terminal's output-quiet detector — never child-process counting,
 *  which mis-reads persistent MCP servers as work in flight. */
export function isSessionIdle(id: string): boolean {
  return isIdleStatus(sessionStatus(id));
}

function settleIdleWaiters(id: string): void {
  const waiters = idleWaiters.get(id);
  if (!waiters) {
    return;
  }

  idleWaiters.delete(id);
  for (const settle of waiters) {
    settle();
  }
}

/** Resolve once the session reaches an idle prompt, exits, or is dropped — the
 *  graceful-leave gate: a deliberate leave (project switch, back to the picker)
 *  waits here before killing, so no mid-flight work is severed. `timeoutMs`
 *  caps the wait so a wedged agent can never trap the user on the way out. */
export function whenSessionIdle({ id, timeoutMs }: {
  id: string;
  timeoutMs: number;
}): Promise<void> {
  if (isSessionIdle(id)) {
    return Promise.resolve();
  }

  return new Promise(resolve => {
    const timer = setTimeout(() => {
      const remaining = (idleWaiters.get(id) ?? []).filter(waiter => waiter !== settle);
      if (remaining.length > 0) {
        idleWaiters.set(id, remaining);
      } else {
        idleWaiters.delete(id);
      }

      resolve();
    }, timeoutMs);

    function settle(): void {
      clearTimeout(timer);
      resolve();
    }

    idleWaiters.set(id, [...(idleWaiters.get(id) ?? []), settle]);
  });
}
