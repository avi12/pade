// Reflect known-task runs the agent starts (SoC: cross-component state in
// lib/stores). PADE never spawned these processes and can't see the agent's
// children — only its PTY text — so a line that looks like an invocation of a
// known task's command is what starts the tracking: it attaches a dock card,
// and the Tasks panel reads "running" off that card.
//
// ONE definition of running, and it is the process. The panel used to keep its
// own flag that cleared when the agent's TURN ended, so `pnpm lint` sat there
// reading RUNNING for as long as the agent kept working afterwards — minutes
// after it had exited. Detection can only ever say a task STARTED; whether it is
// still running is a question about a process, and `runners` already asks it
// (`session_task_running`) to decide when to drop the card. The panel now reads
// that same answer instead of guessing alongside it.
//
// ATTACHING obeys that same law. A fullscreen agent repaints its whole transcript
// for as long as it is on screen, so the `Bash(pnpm journeys)` line of a task that
// finished hours ago keeps arriving as fresh PTY text. Attaching on the text alone
// made the dock card appear, the liveness poll drop it a few seconds later, and
// the next repaint bring it straight back — a card flickering on a ~4s cycle, and
// with it a 132px dock that resized the terminal under the user's cursor every
// time. So detection nominates; the process tree decides.

import { pty } from "@/lib/bridge";
import { attachedRunner, attachRunner, runnerRows, TASK_LIVENESS_POLL_MS } from "@/lib/stores/runners.svelte";
import { taskCatalog, type TaskCatalogSnapshot } from "@/lib/stores/taskCatalog.svelte";
import { isTaskInvocation } from "@/lib/task-detect";
import type { TaskGroup } from "@/lib/types";
import type { UnlistenFn } from "@tauri-apps/api/event";
import { SvelteMap } from "svelte/reactivity";

/** Joins a task key's two parts. NUL can appear in neither a path nor a shell
 *  command, so the composed key can never collide with a real dir/command pair. */
const KEY_SEPARATOR = "\u0000";

/** Unique key for a task: its directory + command (matches the Tasks panel). */
export function taskKey({ directory, command }: {
  directory: string;
  command: string;
}): string {
  return `${directory}${KEY_SEPARATOR}${command}`;
}

/** When each (session, task) was last put to the process tree. The invocation
 *  line arrives on every repaint, so without this the detector would ask the
 *  backend once per PTY chunk; one answer per liveness cadence is as fine-grained
 *  as the question can be usefully asked. */
const lastLivenessCheck = new SvelteMap<string, number>();

/** Whether a task (by key) is currently running (reactive) — a live runner row,
 *  which for an agent-started task means a live process under the agent's tree. */
export function isTaskRunning(key: string): boolean {
  return runnerRows().some(row =>
    !row.done && taskKey({
      directory: row.cwd,
      command: row.command
    }) === key);
}

/** Drop a dead session's throttle entries — nothing can be running under a PTY
 *  that has exited, and a fresh session must be free to ask again at once. */
function forgetSession(sessionId: string): void {
  const prefix = `${sessionId}${KEY_SEPARATOR}`;
  for (const checkKey of lastLivenessCheck.keys()) {
    if (checkKey.startsWith(prefix)) {
      lastLivenessCheck.delete(checkKey);
    }
  }
}

/** One runnable task as the detector needs it: its key + command for matching,
 *  plus the label/kind/dir an attached runner needs to render itself. */
export interface KnownTaskCommand {
  key: string;
  command: string;
  label: string;
  kind: TaskGroup["kind"];
  dir: string;
}

/** Derive detector inputs from the exact catalog snapshot rendered by the panel. */
export function knownTaskCommands(snapshot: TaskCatalogSnapshot): KnownTaskCommand[] {
  return snapshot.groups.flatMap(group =>
    group.tasks.map(task => ({
      key: taskKey({
        directory: group.dir,
        command: task.command
      }),
      command: task.command,
      label: task.name,
      kind: group.kind,
      dir: group.dir
    })));
}

/** Give a detected task a dock card — but only once the OS agrees a process is
 *  actually running it. Never rejects, so the PTY handler can fire it and move
 *  on: a failed check simply means "not running", which is also the safe answer.
 *
 *  It outlives the agent's turn (a dev server keeps running), so nothing clears
 *  it on idle; the liveness poll in `runners` drops the card when the process
 *  exits, and the next real invocation earns a new one. */
async function attachWhenRunning({ sessionId, task }: {
  sessionId: string;
  task: KnownTaskCommand;
}): Promise<void> {
  const alreadyShown = attachedRunner({
    sessionId,
    command: task.command
  }) !== undefined;
  if (alreadyShown) {
    return;
  }

  const checkKey = `${sessionId}${KEY_SEPARATOR}${task.key}`;
  const checkedAt = lastLivenessCheck.get(checkKey);
  const now = performance.now();
  const askedRecently = checkedAt !== undefined && now - checkedAt < TASK_LIVENESS_POLL_MS;
  if (askedRecently) {
    return;
  }

  lastLivenessCheck.set(checkKey, now);
  const running = await pty.sessionTaskRunning({
    id: sessionId,
    command: task.command
  }).catch(() => false);
  if (!running) {
    return;
  }

  attachRunner({
    sessionId,
    label: task.label,
    kind: task.kind,
    command: task.command,
    cwd: task.dir
  });
}

function detect({ sessionId, chunk }: {
  sessionId: string;
  chunk: string;
}): void {
  const lines = chunk.split("\n");
  for (const task of knownTaskCommands(taskCatalog.snapshot)) {
    const isInvocation = lines.some(line => isTaskInvocation({
      line,
      command: task.command
    }));
    if (!isInvocation) {
      continue;
    }

    attachWhenRunning({
      sessionId,
      task
    });
    return;
  }
}

let listenerInitialization: Promise<void> | undefined;
let listenerUnlistens: UnlistenFn[] = [];

async function startListeners(): Promise<void> {
  const pendingUnlistens: UnlistenFn[] = [];
  try {
    pendingUnlistens.push(
      await pty.onData(chunk => detect({
        sessionId: chunk.id,
        chunk: chunk.data
      }))
    );
    pendingUnlistens.push(await pty.onExit(id => forgetSession(id)));
    listenerUnlistens = pendingUnlistens;
  } catch (caughtError) {
    for (const unlisten of pendingUnlistens) {
      unlisten();
    }
    throw caughtError;
  }
}

/** Start watching agent output for known-task runs. Idempotent; call once from
 *  the app shell (like the runner listeners). */
export async function initTaskRunDetection(project: () => string): Promise<void> {
  await taskCatalog.initialize(project);

  if (listenerUnlistens.length === 0) {
    if (!listenerInitialization) {
      listenerInitialization = startListeners();
    }

    try {
      await listenerInitialization;
    } finally {
      listenerInitialization = undefined;
    }
  }
}

/** Re-read task commands after this window switches projects. */
export async function refreshTaskRunDetection(): Promise<void> {
  await taskCatalog.refresh();
}
