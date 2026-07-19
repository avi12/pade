// AI session-naming state machine (SoC: cross-component state in lib/stores).
//
// Clicking a tab's ✦ button toggles auto-naming for that session. When on, the
// rules are:
//   1. generate immediately — but only while the agent is actively working;
//   2. refresh the name every 30s while it keeps working;
//   3. when it stops (goes idle or exits), generate one final name, then pause;
//   4. when work resumes, the 30s refresh restarts.
// The name is produced backend-side from the session's rolling transcript.

import { pty } from "@/lib/bridge";
import { setSessionLabel } from "@/lib/stores/sessionLabels.svelte";
import { sessionStatus } from "@/lib/stores/sessions.svelte";
import { SessionStatus } from "@/lib/types";
import { SvelteMap } from "svelte/reactivity";

const REFRESH_MS = 30_000;

/** Sessions with auto-naming on → the agent command used to name them. */
const naming = new SvelteMap<string, string>();

/** Non-reactive per-session runtime: the refresh timer and last-seen status. */
interface Controller {
  timer?: ReturnType<typeof setInterval>;
  last: SessionStatus;
}
// Plain Map: timers/last-status are runtime bookkeeping, not reactive UI state.
// eslint-disable-next-line svelte/prefer-svelte-reactivity
const controllers = new Map<string, Controller>();

async function generate(id: string): Promise<void> {
  const agent = naming.get(id);
  if (agent === undefined) {
    return;
  }

  try {
    const name = await pty.generateName({
      id,
      agent
    });
    // Only apply if still naming this session (it may have been toggled off).
    if (name !== null && naming.has(id)) {
      setSessionLabel({
        id,
        label: name
      });
    }
  } catch {
    // Naming is best-effort; a failed generate just skips this refresh.
  }
}

/** Whether AI auto-naming is on for a session (reactive). */
export function isNaming(id: string): boolean {
  return naming.has(id);
}

/** Toggle auto-naming. Turning it on names immediately only while the agent is
 *  working (rule 1); the refresh and stop/continue handling live in the effect
 *  below. Turning it off clears the timer and keeps the last label. */
export function toggleNaming({ id, agent }: {
  id: string;
  agent: string;
}): void {
  if (naming.has(id)) {
    dropNaming(id);
    return;
  }

  naming.set(id, agent);
  controllers.set(id, { last: sessionStatus(id) });

  if (sessionStatus(id) === SessionStatus.enum.working) {
    generate(id);
  }
}

/** Stop auto-naming for a session and release its timer (call on tab close). */
export function dropNaming(id: string): void {
  const controller = controllers.get(id);
  if (controller?.timer !== undefined) {
    clearInterval(controller.timer);
  }

  controllers.delete(id);
  naming.delete(id);
}

// Reconcile every named session against its live status: run the 30s refresh
// while working, tear it down when stopped, and fire one final name on the
// working→stopped edge. Re-runs whenever a named session's status changes.
$effect.root(() => {
  $effect(() => {
    for (const id of naming.keys()) {
      const status = sessionStatus(id);
      const controller = controllers.get(id);
      if (controller === undefined) {
        continue;
      }

      const running = status === SessionStatus.enum.working;
      const wasRunning = controller.last === SessionStatus.enum.working;
      if (running && controller.timer === undefined) {
        controller.timer = setInterval(async () => {
          await generate(id);
        }, REFRESH_MS);
      } else if (!running && controller.timer !== undefined) {
        clearInterval(controller.timer);
        controller.timer = undefined;
      }

      if (!running && wasRunning) {
        generate(id);
      }

      controller.last = status;
    }
  });
});
