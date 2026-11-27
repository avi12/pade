// Whether a Claude session has a `/goal` running, read off its screen: Claude
// shows "◎ /goal active" in its footer for as long as one is. A goal keeps the
// agent from ending its turn until the condition holds, so the handoff clears it
// first and sets it again in the successor (handoff-goal).

import { SvelteSet } from "svelte/reactivity";

import { registerSessionState, SessionStateKey } from "@/lib/session/lifecycle/session-scope";

/** What Claude's footer shows while a goal runs. */
const GOAL_ACTIVE_MARKER = "/goal active";

const sessionsWithGoal = new SvelteSet<string>();

registerSessionState({
  key: SessionStateKey.enum.goal,
  state: {
    forget(id) {
      sessionsWithGoal.delete(id);
    }
  }
});

/** Read whether the session's screen shows a running goal. */
export function observeGoalScreen({ id, text }: { id: string; text: string }) {
  const isActive = text.includes(GOAL_ACTIVE_MARKER);
  if (isActive) {
    sessionsWithGoal.add(id);
  } else {
    sessionsWithGoal.delete(id);
  }
}

/** Whether the session has a goal running (reactive). */
export function isGoalActive(id: string) {
  return sessionsWithGoal.has(id);
}
