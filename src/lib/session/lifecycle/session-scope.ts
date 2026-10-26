// Everything PADE keeps per agent session - its name, status, usage, recovery
// loops, ... - registers here once, next to the state it owns. The session's
// lifecycle then reaches all of it from one place: ending a session forgets it
// everywhere, a restart's re-key carries what should outlive it to the new id,
// and a reload or a hand-off to another window saves and restores what should
// survive those. A new piece of per-session state is one registration in its
// own store; no close, restart, reload or hand-off path changes.

import { z } from "zod";

/** The closed set of per-session state, one member per store - also the key a
 *  saved value is filed under in a session snapshot or hand-off. */
export const SessionStateKey = z.enum([
  "label",
  "naming",
  "status",
  "attention",
  "activity",
  "context",
  "mcpReload",
  "apiError",
  "usageLimit",
  "usagePacing",
  "usageAdapt",
  "modelSwitch",
  "retheme"
]);
export type SessionStateKey = z.infer<typeof SessionStateKey>;

/** A session's saved state, keyed by store - each store parses its own value. */
export const SavedSessionState = z.partialRecord(SessionStateKey, z.unknown());
export type SavedSessionState = z.infer<typeof SavedSessionState>;

interface SessionRekey {
  fromId: string;
  toId: string;
}

interface SessionState {
  /** Let go of everything kept for a session that ended or moved away. */
  forget: (id: string) => void;
  /** Move what should outlive a restart onto the id it gives the session. The
   *  old id is forgotten right after, so this only copies. */
  carry?: (rekey: SessionRekey) => void;
  /** What should survive a reload or a hand-off - `undefined` when nothing. */
  save?: (id: string) => unknown;
  /** Take back a value `save` returned, after a reload or a hand-off. */
  restore?: (restoring: {
    id: string;
    saved: unknown;
  }) => void;
}

// Keyed so a store that hot-reloads replaces its registration instead of
// leaving a second one behind.
const registered = new Map<SessionStateKey, SessionState>();

/** Register one store's per-session state. */
export function registerSessionState(key: SessionStateKey, state: SessionState) {
  registered.set(key, state);
}

/** A session ended or moved to another window: forget it everywhere. */
export function forgetSession(id: string) {
  for (const state of registered.values()) {
    state.forget(id);
  }
}

/** A restart gave a session a new id: carry what outlives it, forget the rest. */
export function rekeySession(rekey: SessionRekey) {
  for (const state of registered.values()) {
    state.carry?.(rekey);
  }

  forgetSession(rekey.fromId);
}

/** What a session keeps across a reload or a hand-off. */
export function savedSessionState(id: string) {
  const saved: SavedSessionState = {};
  for (const [key, state] of registered) {
    const value = state.save?.(id);
    if (value !== undefined) {
      saved[key] = value;
    }
  }

  return saved;
}

/** Give a session back what `savedSessionState` kept for it. */
export function restoreSessionState({ id, saved }: {
  id: string;
  saved: SavedSessionState;
}) {
  for (const [key, state] of registered) {
    const value = saved[key];
    if (value !== undefined) {
      state.restore?.({
        id,
        saved: value
      });
    }
  }
}
