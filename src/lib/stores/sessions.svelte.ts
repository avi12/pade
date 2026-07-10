// Shared session-status store (SoC: cross-component state lives in lib/stores).
// The Terminal owns the truth about a PTY's liveness (idle detection etc.) and
// publishes it here; the top-bar tabs read it to show a per-session status dot.
// A SvelteMap keeps reads reactive across components.

import { SessionStatus } from "@/lib/types";
import { SvelteMap } from "svelte/reactivity";

const statuses = new SvelteMap<string, SessionStatus>();

/** Publish the status of a session (called by its Terminal). */
export function setSessionStatus({ id, status }: {
  id: string;
  status: SessionStatus;
}): void {
  statuses.set(id, status);
}

/** Read a session's status (reactive) — defaults to `starting` until known. */
export function sessionStatus(id: string): SessionStatus {
  return statuses.get(id) ?? SessionStatus.enum.starting;
}

/** Forget a session's status when its tab closes. */
export function dropSessionStatus(id: string): void {
  statuses.delete(id);
}
