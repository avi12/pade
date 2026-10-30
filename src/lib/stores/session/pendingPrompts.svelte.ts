// A session's first prompt while it is still waiting to be delivered - the one
// home of it. Launching a session with a prompt files it here; the pane's
// delivery (lib/terminal/input/initial-prompt-delivery) reads it and clears it
// once the agent has really taken it up. Until then it rides along wherever the
// session goes: a reload re-attaches with it, and a hand-off to another window
// carries it there, so a session moved before its prompt went in still gets it.
// A restart drops it - the restarted agent resumes rather than re-sends.

import { registerSessionState, SessionStateKey } from "@/lib/session/lifecycle/session-scope";
import { z } from "zod";

// eslint-disable-next-line svelte/prefer-svelte-reactivity -- read by delivery on output events, never rendered
const pending = new Map<string, string>();

/** File a session's first prompt for delivery - a session launched without one
 *  has nothing pending. */
export function setPendingPrompt({ id, prompt }: {
  id: string;
  prompt: string | undefined;
}) {
  if (prompt === undefined) {
    return;
  }

  pending.set(id, prompt);
}

/** The session's first prompt, while it still waits to be delivered. */
export function pendingPrompt(id: string) {
  return pending.get(id);
}

/** The prompt went in (or delivery gave up on it): nothing is pending anymore. */
export function clearPendingPrompt(id: string) {
  pending.delete(id);
}

registerSessionState(SessionStateKey.enum.pendingPrompt, {
  forget: clearPendingPrompt,
  save: pendingPrompt,
  restore({ id, saved }) {
    const parsed = z.string().min(1).safeParse(saved);
    if (parsed.success) {
      pending.set(id, parsed.data);
    }
  }
});
