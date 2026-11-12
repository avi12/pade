// The in-flight marker's durable twin. The in-memory set dies with the module
// (a window reload, an HMR swap of anything in its import chain), and a scan
// running on the fresh instance re-prompted a session whose handoff request
// was already sitting in the agent's queue - the user watched the same
// request queue twice. sessionStorage survives both resets and still scopes
// to this window's lifetime, matching the sessions themselves.
const HANDOFF_MARKER_PREFIX = "pade-handoff-inflight:";
/** The stored marker value - only its presence is ever read. */
const HANDOFF_MARKER_VALUE = "1";

function persistentMarker(sessionId: string) {
  return `${HANDOFF_MARKER_PREFIX}${sessionId}`;
}

export function hasPersistentMarker(sessionId: string) {
  return sessionStorage.getItem(persistentMarker(sessionId)) !== null;
}

export function storePersistentMarker(sessionId: string) {
  sessionStorage.setItem(persistentMarker(sessionId), HANDOFF_MARKER_VALUE);
}

export function clearPersistentMarker(sessionId: string) {
  sessionStorage.removeItem(persistentMarker(sessionId));
}
