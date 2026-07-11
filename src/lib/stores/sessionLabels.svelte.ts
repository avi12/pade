// Per-session display-label overrides (SoC: cross-component state lives in
// lib/stores). A session tab shows its agent's default label until an AI or
// manual rename sets a custom one here; the map keeps reads reactive. Ephemeral
// — dropped when the tab closes.

import { SvelteMap } from "svelte/reactivity";

const labels = new SvelteMap<string, string>();

/** Set a session's custom display label (overrides the agent's default). */
export function setSessionLabel({ id, label }: {
  id: string;
  label: string;
}): void {
  labels.set(id, label);
}

/** Read a session's custom label, or undefined if none is set (reactive). */
export function sessionLabel(id: string): string | undefined {
  return labels.get(id);
}

/** Forget a session's label when its tab closes. */
export function dropSessionLabel(id: string): void {
  labels.delete(id);
}
