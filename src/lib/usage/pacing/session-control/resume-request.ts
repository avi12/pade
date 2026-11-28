// What pacing types into a session it stopped, once the pause is over. A bare
// "continue" leaves the agent believing the user interrupted it - Claude
// reports a stop-all as "stopped by the user" - so it asks before restarting
// the subagents the stop killed, and the work waits on a question nobody
// meant to raise. The request says who paused it, that the pause is over,
// and that each stopped subagent's task is to be restarted, unasked.

/** A subagent the terminal stop ended, as its transcript names it. */
export interface StoppedSubagent {
  id: string;
  description: string | null;
}

const PAUSE_LIFTED =
  "PADE's usage pacing paused this session to keep it within the usage limits, and the pause is now over. " +
  "The interruption came from PADE, not from the user.";
const CARRY_ON = "Carry on from where you stopped, without asking for confirmation.";
const SUBAGENTS_RESTART =
  "They can't be resumed: check what each one left behind, then start a new subagent for each to finish its task from the current state.";

function quoted(description: string) {
  return `"${description}"`;
}

/** The sentence naming the subagents the stop ended - every one by its task
 *  when the transcripts named them all. */
function stoppedSubagentsSentence(subagents: readonly StoppedSubagent[]) {
  const descriptions = subagents.flatMap(subagent => (subagent.description === null ? [] : [subagent.description]));
  const isEveryOneNamed = descriptions.length === subagents.length && descriptions.length > 0;
  if (isEveryOneNamed) {
    return `It also stopped these background subagents mid-task: ${descriptions.map(quoted).join(", ")}.`;
  }

  return "It also stopped your background subagents mid-task.";
}

/** The request that picks a paused session back up. `hasStoppedSubagents`
 *  covers a stop-all whose subagents no transcript named. */
export function resumeRequest({
  subagents,
  hasStoppedSubagents
}: {
  subagents: readonly StoppedSubagent[];
  hasStoppedSubagents: boolean;
}) {
  const isAnySubagentStopped = hasStoppedSubagents || subagents.length > 0;
  if (!isAnySubagentStopped) {
    return `${PAUSE_LIFTED} ${CARRY_ON}`;
  }

  return [PAUSE_LIFTED, stoppedSubagentsSentence(subagents), SUBAGENTS_RESTART, CARRY_ON].join(" ");
}
