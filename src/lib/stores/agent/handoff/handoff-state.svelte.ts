// What one auto-handoff scope remembers: the sessions mid-handoff (in memory and
// as a persistent marker that survives a reload), how many times each was asked
// for its doc, and the status line to show. The handoff steps read and write it
// only through here.

import { SvelteMap, SvelteSet } from "svelte/reactivity";

import {
  clearPersistentMarker,
  hasPersistentMarker,
  storePersistentMarker
} from "@/lib/stores/agent/handoff/handoff-markers";

export function createHandoffState() {
  const handingOff = new SvelteSet<string>();
  const handoffAttempts = new SvelteMap<string, number>();
  let note = $state("");
  // The scan's own line for a session over threshold that it holds back because
  // the agent still has work running; an in-flight handoff's `note` wins over it.
  let deferredNote = $state("");

  function markHandingOff(sessionId: string) {
    handingOff.add(sessionId);
    storePersistentMarker(sessionId);
  }

  function unmarkHandingOff(sessionId: string) {
    handingOff.delete(sessionId);
    clearPersistentMarker(sessionId);
  }

  return {
    handingOff,
    handoffAttempts,
    /** The status line while a handoff is in flight ("" when idle). */
    get note() {
      return note;
    },
    set note(next) {
      note = next;
    },
    get deferredNote() {
      return deferredNote;
    },
    set deferredNote(next) {
      deferredNote = next;
    },
    markHandingOff,
    unmarkHandingOff,
    isHandingOff(sessionId: string) {
      return handingOff.has(sessionId) || hasPersistentMarker(sessionId);
    },
    // Give up on this handoff for now, leaving the session free for a later scan.
    abandonHandoff(sessionId: string) {
      unmarkHandingOff(sessionId);
      note = "";
    }
  };
}

export type HandoffState = ReturnType<typeof createHandoffState>;
