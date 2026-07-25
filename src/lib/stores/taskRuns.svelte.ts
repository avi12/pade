// Reflect known-task runs the agent starts (SoC: cross-component state in
// lib/stores). We can't see the agent's child processes — only its PTY text —
// so this is best-effort: when a line in a session's output looks like an
// invocation of a known task's command, that task shows as "running" in the
// Tasks panel. It clears when the session goes idle (its agent turn ends) or
// exits. No process control — purely a status reflection.
//
// Limitation: a quiet long-running task (e.g. a dev server) clears once the
// agent's terminal stops producing output; one-shot tasks (build/test/lint)
// reflect accurately.

import { pty } from "@/lib/bridge";
import { runnerRows } from "@/lib/stores/runners.svelte";
import { sessionStatus } from "@/lib/stores/sessions.svelte";
import { taskCatalog, type TaskCatalogSnapshot } from "@/lib/stores/taskCatalog.svelte";
import { isTaskInvocation } from "@/lib/task-detect";
import { SessionStatus } from "@/lib/types";
import type { UnlistenFn } from "@tauri-apps/api/event";
import { SvelteMap, SvelteSet } from "svelte/reactivity";

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

/** Task keys currently reflected as running. */
const running = new SvelteSet<string>();
/** The task each session is currently running, so we can clear on idle/exit. */
const bySession = new SvelteMap<string, string>();

/** Whether a task (by key) is currently running (reactive). */
export function isTaskRunning(key: string): boolean {
  return running.has(key) || runnerRows().some(row =>
    !row.done && taskKey({
      directory: row.cwd,
      command: row.command
    }) === key);
}

function clearSessionTask(sessionId: string): void {
  const key = bySession.get(sessionId);
  if (key === undefined) {
    return;
  }

  bySession.delete(sessionId);
  // Only drop the running flag if no other session is running the same task.
  const stillRunning = [...bySession.values()].includes(key);
  if (!stillRunning) {
    running.delete(key);
  }
}

function markSessionTask({ sessionId, key }: {
  sessionId: string;
  key: string;
}): void {
  if (bySession.get(sessionId) === key) {
    return;
  }

  clearSessionTask(sessionId); // one foreground task per session
  bySession.set(sessionId, key);
  running.add(key);
}

/** Derive detector inputs from the exact catalog snapshot rendered by the panel. */
export function knownTaskCommands(snapshot: TaskCatalogSnapshot): {
  key: string;
  command: string;
}[] {
  return snapshot.groups.flatMap(group =>
    group.tasks.map(task => ({
      key: taskKey({
        directory: group.dir,
        command: task.command
      }),
      command: task.command
    })));
}

function detect({ sessionId, chunk }: {
  sessionId: string;
  chunk: string;
}): void {
  const lines = chunk.split("\n");
  for (const { key, command } of knownTaskCommands(taskCatalog.snapshot)) {
    const isInvocation = lines.some(line => isTaskInvocation({
      line,
      command
    }));
    if (!isInvocation) {
      continue;
    }

    markSessionTask({
      sessionId,
      key
    });
    return;
  }
}

// Clear a session's running task once its agent turn ends (ready) or it exits.
$effect.root(() => {
  $effect(() => {
    for (const sessionId of bySession.keys()) {
      const status = sessionStatus(sessionId);
      if (status === SessionStatus.enum.ready || status === SessionStatus.enum.exited) {
        clearSessionTask(sessionId);
      }
    }
  });
});

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
    pendingUnlistens.push(await pty.onExit(id => clearSessionTask(id)));
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
