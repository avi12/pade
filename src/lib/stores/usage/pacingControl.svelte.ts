// The one way usage pacing writes a session's pacing control - the file the
// agent's hook reads on every tool call (agents/running_model/pacing_control).
// Each part has one owner: the session's own window writes its subagent cap
// (lib/stores/usage/usageAdapt) and whether it is being stopped
// (lib/stores/usage/usagePacing), and when its rest ends, which holds its tool
// calls; the account-wide round's leader writes every
// session's throttle (lib/stores/usage/usageAccountRound). A part is sent only
// when it changes, so a round that repeats itself writes nothing.
//
// It also keeps the latest round's throttles for the usage panel, in every
// window - the leader's plan reaches them all.

import { SvelteMap } from "svelte/reactivity";

import { Command, sendMessage } from "@/lib/ipc/messaging";
import type { CommandProtocolMap } from "@/lib/ipc/messaging";
import type { AgentSession } from "@/lib/types/agent";
import { PacingControlPartKind } from "@/lib/types/pacing";
import type { PacingRestUntil, PacingThrottle } from "@/lib/types/pacing";
import type { ThrottledSession } from "@/lib/usage/pacing/engine/account-round";

/** One part of a session's control, as the backend takes it. */
type ControlPart = Parameters<CommandProtocolMap[typeof Command.AGENT_SET_PACING_CONTROL]>[0]["part"];

/** Who a control belongs to: the agent that reads it, and its conversation. */
export type ControlTarget = Pick<AgentSession, "conversationId"> & {
  command: string;
};

/** What each part was last set to, by conversation and part. */
const written = new SvelteMap<string, string>();

function writtenKey({ conversationId, part }: { conversationId: string; part: ControlPart["part"] }) {
  return `${conversationId}:${part}`;
}

/** Set one part of a session's control. A session that hasn't started its
 *  conversation has nothing for an agent to read yet. Never rejects: a failed
 *  write is retried the next time the part is set. */
export async function setControlPart({ target, part }: { target: ControlTarget; part: ControlPart }) {
  const conversationId = target.conversationId;
  if (conversationId === undefined) {
    return;
  }

  const key = writtenKey({
    conversationId,
    part: part.part
  });
  const value = JSON.stringify(part.value);
  if (written.get(key) === value) {
    return;
  }

  written.set(key, value);
  try {
    await sendMessage(Command.AGENT_SET_PACING_CONTROL, {
      command: target.command,
      conversationId,
      part
    });
  } catch (error) {
    written.delete(key);
    console.warn("pade: could not write the session's pacing control", error);
  }
}

/** Set whether a session is being stopped. */
export async function setStopPart({ target, isStopped }: { target: ControlTarget; isStopped: boolean }) {
  await setControlPart({
    target,
    part: {
      part: PacingControlPartKind.enum.stop,
      value: isStopped
    }
  });
}

/** Set when a session's rest ends, its tool calls held until then; null lifts
 *  the rest. */
export async function setRestPart({ target, restUntil }: { target: ControlTarget; restUntil: PacingRestUntil }) {
  await setControlPart({
    target,
    part: {
      part: PacingControlPartKind.enum.rest,
      value: restUntil
    }
  });
}

/** The throttle that holds nothing back - writing it lifts one. */
export const NO_THROTTLE: PacingThrottle = {
  isBlockingSpawns: false,
  subagents: {}
};

/** Set a session's throttle; an empty one lifts it. */
export async function setThrottlePart({ target, throttle }: { target: ControlTarget; throttle: PacingThrottle }) {
  await setControlPart({
    target,
    part: {
      part: PacingControlPartKind.enum.throttle,
      value: throttle
    }
  });
}

let throttles = $state.raw<readonly ThrottledSession[]>([]);

/** The latest round's throttled sessions, across every window (reactive). */
export function throttledSessions() {
  return throttles;
}

export function showThrottles(next: readonly ThrottledSession[]) {
  throttles = next;
}
