// AI session-naming state machine (SoC: cross-component state in lib/stores).
//
// Auto-naming is turned on per session, either by the tab's ✦ button or in bulk
// by the window's setting (lib/ai-naming-scope decides that from the project and
// global preferences; the app shell applies it here). When on, the rules are:
//   1. generate immediately - but only while the agent is actively working;
//   2. refresh the name every 30s while it keeps working;
//   3. when it stops (goes idle or exits), generate one final name, then pause;
//   4. when work resumes, the 30s refresh restarts.
// Every generate is throttled to at most once per 30s per session, so a flurry of
// status edges - a quick stop/resume, or a status flicker while a long task runs -
// can never rename faster than that cadence (rule 3's stop-name is skipped when it
// falls within 30s of the last). The name is produced backend-side from the
// session's rolling transcript.

import { pty } from "@/lib/bridge";
import { setSessionLabel } from "@/lib/stores/sessionLabels.svelte";
import { sessionStatus } from "@/lib/stores/sessions.svelte";
import { SessionStatus } from "@/lib/types";
import { SvelteMap } from "svelte/reactivity";

const REFRESH_MS = 30_000;
/** A session's `lastGeneratedAt` before its first name, so that one always runs. */
const NEVER_GENERATED = 0;

/** Sessions with auto-naming on → the agent command used to name them. */
const naming = new SvelteMap<string, string>();

/** Non-reactive per-session runtime: the refresh timer, last-seen status, and the
 *  time of the last name generated (to throttle every generate to the 30s cadence). */
interface Controller {
  timer?: ReturnType<typeof setInterval>;
  lastStatus: SessionStatus;
  lastGeneratedAt: number;
}
// Plain Map: timers/last-status are runtime bookkeeping, not reactive UI state.
// eslint-disable-next-line svelte/prefer-svelte-reactivity
const controllers = new Map<string, Controller>();

/** The session's controller when a name is due, or undefined while throttled:
 *  at most one name per 30s per session, whatever triggered it (toggle-on, the
 *  interval, or a stop edge). */
function dueController(id: string) {
  const controller = controllers.get(id);
  const isDue = controller !== undefined && Date.now() - controller.lastGeneratedAt >= REFRESH_MS;
  return isDue ? controller : undefined;
}

/** Label the session with a generated name, unless naming was toggled off while
 *  the request ran. */
function applyGeneratedName({ id, name }: {
  id: string;
  name: string | null;
}) {
  if (name === null || !naming.has(id)) {
    return;
  }

  setSessionLabel({
    id,
    label: name
  });
}

async function generate(id: string) {
  const agent = naming.get(id);
  const controller = dueController(id);
  if (agent === undefined || controller === undefined) {
    return;
  }

  // Stamp the time up front so a concurrent trigger during the in-flight request
  // can't slip a second one in.
  controller.lastGeneratedAt = Date.now();
  try {
    applyGeneratedName({
      id,
      name: await pty.generateName({
        id,
        agent
      })
    });
  } catch (error) {
    console.warn("pade: could not generate a session name, keeping the current one", error);
  }
}

/** Start naming a session with `agent`, as if it had never been named. */
function startNaming({ id, agent }: {
  id: string;
  agent: string;
}) {
  naming.set(id, agent);
  controllers.set(id, {
    lastStatus: sessionStatus(id),
    lastGeneratedAt: NEVER_GENERATED
  });
}

function stopRefresh(controller: Controller) {
  clearInterval(controller.timer);
  controller.timer = undefined;
}

/** Whether AI auto-naming is on for a session (reactive). */
export function isNaming(id: string) {
  return naming.has(id);
}

/** Turn auto-naming on or off for one session. Turning it on names immediately
 *  only while the agent is working (rule 1); the refresh and stop/continue
 *  handling live in the effect below. Turning it off clears the timer and keeps
 *  the last label. Idempotent, so re-applying the same setting is free. */
export function setNaming({ id, agent, isOn }: {
  id: string;
  agent: string;
  isOn: boolean;
}) {
  if (!isOn) {
    dropNaming(id);
    return;
  }

  if (naming.has(id)) {
    return;
  }

  startNaming({
    id,
    agent
  });

  if (sessionStatus(id) === SessionStatus.enum.working) {
    generate(id);
  }
}

/** Flip auto-naming for one session - what the tab's ✦ button does. */
export function toggleNaming({ id, agent }: {
  id: string;
  agent: string;
}) {
  setNaming({
    id,
    agent,
    isOn: !naming.has(id)
  });
}

/** Apply the window's naming setting to every session it currently holds. This
 *  is deliberately a bulk assignment, not a default that per-tab ✦ toggles layer
 *  on top of: changing the setting is the user saying "these tabs", so it also
 *  overrules a tab they had flipped by hand. */
export function applyNamingToAll({ sessions, isOn }: {
  sessions: readonly {
    id: string;
    agent: {
      command: string;
    };
  }[];
  isOn: boolean;
}) {
  for (const session of sessions) {
    setNaming({
      id: session.id,
      agent: session.agent.command,
      isOn
    });
  }
}

/** Carry a session's auto-naming toggle to a new id when the session is reset in
 *  place under a fresh id - a restart re-key or a handoff to a successor - so the
 *  ✦ toggle the user turned on survives the reset instead of silently reverting to
 *  off. The successor's OWN agent command is stored (it can differ from the
 *  predecessor on a handoff crossover, e.g. Claude→Codex), and the old id's
 *  controller + refresh timer are released; the reconcile effect re-arms the timer
 *  for the new id on its next status tick. No-op when the old id wasn't naming. */
export function transferNaming({ fromId, toId, agent }: {
  fromId: string;
  toId: string;
  agent: string;
}) {
  if (!naming.has(fromId)) {
    return;
  }

  dropNaming(fromId);
  startNaming({
    id: toId,
    agent
  });
}

/** Stop auto-naming for a session and release its timer (call on tab close). */
export function dropNaming(id: string) {
  const controller = controllers.get(id);
  if (controller !== undefined) {
    stopRefresh(controller);
  }

  controllers.delete(id);
  naming.delete(id);
}

/** Run the 30s refresh while the session works, tear it down once it stops. */
function syncRefresh({ id, controller, isWorking }: {
  id: string;
  controller: Controller;
  isWorking: boolean;
}) {
  const isRefreshing = controller.timer !== undefined;
  if (isWorking === isRefreshing) {
    return;
  }

  if (isWorking) {
    controller.timer = setInterval(async () => {
      await generate(id);
    }, REFRESH_MS);
  } else {
    stopRefresh(controller);
  }
}

/** Bring one named session's refresh in line with its status, and fire one final
 *  name on the working→stopped edge. */
function reconcileNaming({ id, status }: {
  id: string;
  status: SessionStatus;
}) {
  const controller = controllers.get(id);
  if (controller === undefined) {
    return;
  }

  const isWorking = status === SessionStatus.enum.working;
  const wasWorking = controller.lastStatus === SessionStatus.enum.working;
  syncRefresh({
    id,
    controller,
    isWorking
  });

  if (wasWorking && !isWorking) {
    generate(id);
  }

  controller.lastStatus = status;
}

// Reconcile every named session against its live status. Re-runs whenever a
// named session's status changes - the status is read here, before anything can
// bail, so every named session stays a dependency of the effect.
$effect.root(() => {
  $effect(() => {
    for (const id of naming.keys()) {
      reconcileNaming({
        id,
        status: sessionStatus(id)
      });
    }
  });
});
