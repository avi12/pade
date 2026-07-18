// Per-session "attention" state: whether the running agent has put up a
// multiple-choice question and is waiting on the user's pick (SoC: cross-
// component state lives in lib/stores). When one is pending on a tab that isn't
// in front, SessionTabs flashes that tab's status indicator red to grab
// attention. Detection sniffs the PTY stream directly through the shared bridge
// channel — the same "watch a chunk of output" shape as the usage-limit /
// API-error sniffers — because Terminal owns the render path and this store only
// needs to watch for the prompt, never to touch the terminal.

import { pty } from "@/lib/bridge";
import { detectChoicePrompt } from "@/lib/choice-prompt";
import { sessionStatus } from "@/lib/stores/sessions.svelte";
import { SessionStatus } from "@/lib/types";
import { SvelteMap, SvelteSet } from "svelte/reactivity";

/** Sessions with a multiple-choice prompt pending (reactive, so the tab UI
 *  updates as it is set and cleared). */
const awaiting = new SvelteMap<string, boolean>();

// Whether a flagged session has since settled to `ready`. A later `working` then
// reads as "the user answered", as opposed to the working burst that PAINTED the
// prompt (which is when we flag it) — that must not clear the flag. Read only by
// the reconcile transition, never the rendered UI (a SvelteSet so the store keeps
// one reactive-collection type; nothing subscribes to it).
const settledReady = new SvelteSet<string>();

let listening = false;

/** Subscribe once to the PTY stream and flag any session that shows a choice
 *  prompt (call from App onMount, mirroring `ensureRunnerListeners`). A TUI
 *  repaints its prompt every frame, so a session already flagged is left alone. */
export async function ensureChoiceAttention(): Promise<void> {
  if (listening) {
    return;
  }

  listening = true;
  await pty.onData(({ id, data }) => {
    if (awaiting.get(id) || !detectChoicePrompt(data)) {
      return;
    }

    awaiting.set(id, true);
    settledReady.delete(id);
  });
}

/** Whether a session is waiting on the user to answer a multiple-choice prompt
 *  (reactive). */
export function awaitingChoice(id: string): boolean {
  return awaiting.get(id) ?? false;
}

function clear(id: string): void {
  awaiting.delete(id);
  settledReady.delete(id);
}

/** Reconcile the pending flag against the session's live status and whether it is
 *  the one in front — call from an `$effect` so it re-runs as focus / status
 *  change. It stops the flashing when:
 *   - the tab is the active one (the user is now looking at it), or
 *   - the agent went back to `working` after settling `ready` (answered), or
 *   - the session exited.
 *  Otherwise (idle at the prompt) the flag stays and the tab keeps flashing. */
export function reconcileChoiceAttention({ id, isActive }: {
  id: string;
  isActive: boolean;
}): void {
  if (!awaiting.get(id)) {
    return;
  }

  if (isActive) {
    clear(id);
    return;
  }

  const status = sessionStatus(id);
  if (status === SessionStatus.enum.ready) {
    settledReady.add(id);
    return;
  }

  const answered = status === SessionStatus.enum.working && settledReady.has(id);
  if (answered || status === SessionStatus.enum.exited) {
    clear(id);
  }
}

/** Forget a session's attention state when its tab closes. */
export function dropChoiceAttention(id: string): void {
  clear(id);
}
