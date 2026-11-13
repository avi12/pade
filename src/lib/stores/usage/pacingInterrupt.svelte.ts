// Usage pacing's terminal stop: when pacing stops a session mid-turn and the
// session can't take the stop through its hook - it was launched before the
// hook existed, or the hook never fired within a short grace - PADE presses
// the agent's own keys in its terminal (lib/agent/interrupt-keys): the
// stop-all-agents action while a subagent runs (Esc leaves background
// subagents running), then the interrupt. It then checks the subagents'
// transcripts: one still writing gets one more press, and is then listed in
// the usage panel for the user to stop by hand, never pressed in a loop.
// The decisions are lib/usage/pacing/session-control/terminal-stop; this runs them.

import { SvelteMap } from "svelte/reactivity";

import { interruptKeys } from "@/lib/agent/interrupt-keys";
import type { InterruptKeys, KeyPress } from "@/lib/agent/interrupt-keys";
import { Command, sendMessage } from "@/lib/ipc/messaging";
import { sessionLabel } from "@/lib/stores/session/sessionLabels.svelte";
import type { AgentSession } from "@/lib/types/agent";
import { RUNNING_SUBAGENT_MS, subagentsWrittenSince } from "@/lib/usage/pacing/session-control/measured-burn";
import { isHoldFallbackDue } from "@/lib/usage/pacing/session-control/rest-hold";
import {
  interruptPresses,
  isHookTakingStop,
  MAXIMUM_STOP_ATTEMPTS,
  stillWritingSubagents,
  StopVerdict,
  stopVerdict
} from "@/lib/usage/pacing/session-control/terminal-stop";
import { afterDelay } from "@/lib/util/async/delay";

/** How long a session's hook has to take the stop on its next tool call. */
const HOOK_GRACE_MS = 10_000;
/** How long a session's hook has to note it is holding a resting session's
 *  call. Longer than the stop's: a session thinking or writing a long answer
 *  makes no tool call for a while, and falling back too soon would kill an
 *  agent a hold was meant to spare. A session that predates the hold simply
 *  works on for this long before the stop takes over. */
const REST_HOOK_GRACE_MS = 45_000;
/** How long the keys get to stop a subagent before its transcript is read. */
const VERIFY_GRACE_MS = 15_000;
/** Between the keystrokes of one chord - each its own write, as a person types
 *  them, so the agent reads two keys and not one pasted burst. */
const KEYSTROKE_GAP_MS = 150;
/** Between presses of a shortcut pressed twice - well inside each agent's
 *  confirm window (Claude 3 s, OpenCode 5 s). */
const PRESS_GAP_MS = 600;
/** After a stop-all, before the interrupt: the agent starts a turn to say its
 *  subagents were stopped, and the interrupt is what ends it. */
const STOP_ALL_SETTLE_MS = 1500;

/** A subagent the keys couldn't stop, as the usage panel names it. */
interface UnstoppedSubagent {
  id: string;
  description: string | null;
}

/** Each session's subagents the terminal stop gave up on (reactive). */
const unstopped = new SvelteMap<
  string,
  {
    sessionName: string;
    subagents: readonly UnstoppedSubagent[];
  }
>();
/** Each session's current run, so a newer stop or a resume ends an older one. */
const runs = new SvelteMap<string, symbol>();

/** The subagents pacing couldn't stop, per session, across this window. */
export function unstoppedSubagents() {
  return [...unstopped].map(([sessionId, entry]) => ({
    sessionId,
    ...entry
  }));
}

/** Drop a session's terminal stop: it was resumed, or it is gone. */
export function forgetTerminalStop(id: string) {
  runs.delete(id);
  unstopped.delete(id);
}

async function press({ id, keyPress }: { id: string; keyPress: KeyPress }) {
  for (let i = 0; i < keyPress.presses; i++) {
    if (i > 0) {
      await afterDelay(PRESS_GAP_MS);
    }

    for (const keystroke of keyPress.keystrokes) {
      await sendMessage(Command.PTY_WRITE, {
        id,
        data: keystroke
      });
      await afterDelay(KEYSTROKE_GAP_MS);
    }
  }
}

async function pressAll({ id, presses }: { id: string; presses: readonly KeyPress[] }) {
  for (const [i, keyPress] of presses.entries()) {
    if (i > 0) {
      await afterDelay(STOP_ALL_SETTLE_MS);
    }

    await press({
      id,
      keyPress
    });
  }
}

async function hookStopSeenAt(conversationId: string | undefined) {
  if (conversationId === undefined) {
    return null;
  }

  try {
    return await sendMessage(Command.AGENT_PACING_STOP_SEEN, { conversationId });
  } catch (error) {
    console.warn("pade: could not read whether the hook took the stop", error);
    return null;
  }
}

/** Wait out the grace, then whether a rest the hook should hold through is
 *  ignored by it: the session still works and its hook noted no held call since
 *  the rest was written (`restWrittenAt`, epoch ms) - it predates the hold. */
export async function isRestIgnoredByHook({
  conversationId,
  restWrittenAt,
  isWorking
}: {
  conversationId: string | undefined;
  restWrittenAt: number;
  isWorking: () => boolean;
}) {
  await afterDelay(REST_HOOK_GRACE_MS);
  return isHoldFallbackDue({
    isWorking: isWorking(),
    stopSeenAt: await hookStopSeenAt(conversationId),
    restWrittenAt
  });
}

/** What a terminal stop works from. The probes are re-read at every step, so
 *  a session resumed or gone quiet meanwhile is left alone. */
interface TerminalStopRequest {
  session: AgentSession;
  /** When pacing wrote the stop the hook would take, epoch ms. */
  stopWrittenAt: number;
  isStillStopping: () => boolean;
  isWorking: () => boolean;
  /** The agent's screen lists background agents. */
  hasBackgroundAgents: () => boolean;
}

/** Wait out the grace, then whether the session's own hook took the stop. */
async function isTakenByHook({ request, keys }: { request: TerminalStopRequest; keys: InterruptKeys }) {
  if (!keys.hasMidTurnStopHook) {
    return false;
  }

  await afterDelay(HOOK_GRACE_MS);
  return isHookTakingStop({
    keys,
    stopSeenAt: await hookStopSeenAt(request.session.conversationId),
    stopWrittenAt: request.stopWrittenAt
  });
}

/** Whether the session has a subagent running: the screen lists background
 *  agents, or a subagent's transcript was written lately - each alone misses
 *  some, and the interrupt alone leaves every one of them running. */
export async function hasRunningSubagents({
  session,
  hasBackgroundAgents
}: {
  session: AgentSession;
  hasBackgroundAgents: boolean;
}) {
  if (hasBackgroundAgents) {
    return true;
  }

  const running = await subagentsWrittenSince({
    session,
    since: Date.now() - RUNNING_SUBAGENT_MS
  });
  return running.length > 0;
}

/** One press of the keys, then the subagents still writing after it - none
 *  when no subagent was running. */
async function pressAndVerify({ request, keys }: { request: TerminalStopRequest; keys: InterruptKeys }) {
  const { session } = request;
  const isAnySubagentRunning = await hasRunningSubagents({
    session,
    hasBackgroundAgents: request.hasBackgroundAgents()
  });
  const interruptedAt = Date.now();
  await pressAll({
    id: session.id,
    presses: interruptPresses({
      keys,
      hasRunningSubagents: isAnySubagentRunning,
      isWorking: request.isWorking()
    })
  });

  if (!isAnySubagentRunning) {
    return [];
  }

  await afterDelay(VERIFY_GRACE_MS);
  return stillWritingSubagents({
    subagents: await subagentsWrittenSince({
      session,
      since: interruptedAt
    }),
    interruptedAt
  });
}

/** Stop the session through its terminal unless its hook takes the stop
 *  first: press, verify, press once more, then list what still runs for the
 *  user. Never rejects. */
export async function stopThroughTerminal(request: TerminalStopRequest) {
  const { session } = request;
  const id = session.id;
  const run = Symbol(id);
  runs.set(id, run);
  unstopped.delete(id);
  function isCurrent() {
    return runs.get(id) === run && request.isStillStopping();
  }

  try {
    const keys = await interruptKeys(session.agent.command);
    if (keys === null) {
      return;
    }

    // The hook taking the stop ends the turn it refused, but a subagent still
    // writing outlives that - so the keys still go in while one runs.
    const isTakenByHookAlone =
      (await isTakenByHook({
        request,
        keys
      })) &&
      !(await hasRunningSubagents({
        session,
        hasBackgroundAgents: request.hasBackgroundAgents()
      }));
    if (isTakenByHookAlone) {
      return;
    }

    for (let attempt = 1; attempt <= MAXIMUM_STOP_ATTEMPTS && isCurrent(); attempt++) {
      const stillWriting = await pressAndVerify({
        request,
        keys
      });
      const verdict = stopVerdict({
        stillWritingCount: stillWriting.length,
        attempt
      });
      const isGivenUp = verdict === StopVerdict.enum.unstoppable && runs.get(id) === run;
      if (isGivenUp) {
        unstopped.set(id, {
          sessionName: sessionLabel(id) ?? session.agent.label,
          subagents: stillWriting.map(({ id: subagentId, description }) => ({
            id: subagentId,
            description
          }))
        });
      }

      if (verdict !== StopVerdict.enum.retry) {
        return;
      }
    }
  } catch (error) {
    console.warn("pade: could not stop the session through its terminal", error);
  }
}
