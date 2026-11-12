// What one auto-handoff scope remembers: the sessions mid-handoff (in memory and
// as a persistent marker that survives a reload), how many times each was asked
// for its doc, and the status line to show. The handoff steps read and write it
// only through here.

import {
  clearPersistentMarker,
  hasPersistentMarker,
  storePersistentMarker
} from "@/lib/stores/agent/handoff/handoff-markers";
import { SvelteMap, SvelteSet } from "svelte/reactivity";

export class HandoffState {
  handingOff = new SvelteSet<string>();
  /** The status line while a handoff is in flight ("" when idle). */
  note = $state("");
  // The scan's own line for a session over threshold that it holds back because
  // the agent still has work running; an in-flight handoff's `note` wins over it.
  deferredNote = $state("");
  handoffAttempts = new SvelteMap<string, number>();

  markHandingOff(sessionId: string) {
    this.handingOff.add(sessionId);
    storePersistentMarker(sessionId);
  }

  unmarkHandingOff(sessionId: string) {
    this.handingOff.delete(sessionId);
    clearPersistentMarker(sessionId);
  }

  isHandingOff(sessionId: string) {
    return this.handingOff.has(sessionId) || hasPersistentMarker(sessionId);
  }

  // Give up on this handoff for now, leaving the session free for a later scan.
  abandonHandoff(sessionId: string) {
    this.unmarkHandingOff(sessionId);
    this.note = "";
  }
}
