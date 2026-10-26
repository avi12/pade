// Per-session display-label overrides (SoC: cross-component state lives in
// lib/stores). A session tab shows its agent's default label until an AI or
// manual rename sets a custom one here; the map keeps reads reactive. The label
// follows the session through a restart, a reload and a hand-off, and is
// forgotten when the session ends (session-scope).

import { registerSessionState, SessionStateKey } from "@/lib/session/lifecycle/session-scope";
import { SvelteMap } from "svelte/reactivity";
import { z } from "zod";

const labels = new SvelteMap<string, string>();

/** Set a session's custom display label (overrides the agent's default). */
export function setSessionLabel({ id, label }: {
  id: string;
  label: string;
}) {
  labels.set(id, label);
}

/** Read a session's custom label, or undefined if none is set (reactive). */
export function sessionLabel(id: string) {
  return labels.get(id);
}

registerSessionState(SessionStateKey.enum.label, {
  forget(id) {
    labels.delete(id);
  },
  carry({ fromId, toId }) {
    const label = labels.get(fromId);
    if (label !== undefined) {
      labels.set(toId, label);
    }
  },
  save: id => labels.get(id),
  restore({ id, saved }) {
    const parsed = z.string().safeParse(saved);
    if (parsed.success) {
      labels.set(id, parsed.data);
    }
  }
});
