// What a workspace does once its last session has ended - whether the user
// closed the tab or the agent quit on its own. Pure, so the policy reads in one
// place and is unit-tested; App.svelte carries the chosen outcome out.

/** A self-exit this soon after launch reads as a failed start, so the agent is
 *  not respawned into a loop. */
export const RESPAWN_MIN_LIFETIME_MS = 2_000;

/** Where an emptied workspace goes next. */
export const EmptyWorkspaceOutcome = {
  /** Back to the project picker: an unnamed temp workspace has nothing to keep. */
  picker: "picker",
  /** A fresh session of the same agent, so a real project stays usable. */
  respawn: "respawn",
  /** The agent picker, after a start that failed. */
  onboarding: "onboarding"
} as const;

/** Whether a session that exited on its own died at launch. */
export function didFailToStart({ launchedAt, now }: {
  launchedAt: number;
  now: number;
}) {
  return now - launchedAt <= RESPAWN_MIN_LIFETIME_MS;
}

export function emptyWorkspaceOutcome({ isUnnamedTemp, isFailedStart }: {
  isUnnamedTemp: boolean;
  isFailedStart: boolean;
}) {
  if (isUnnamedTemp) {
    return EmptyWorkspaceOutcome.picker;
  }

  if (isFailedStart) {
    return EmptyWorkspaceOutcome.onboarding;
  }

  return EmptyWorkspaceOutcome.respawn;
}
