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

import { feed, pty, tasks as tasksApi } from "@/lib/bridge";
import { baseName } from "@/lib/paths";
import { runnerRows } from "@/lib/stores/runners.svelte";
import { sessionStatus } from "@/lib/stores/sessions.svelte";
import { isTaskInvocation } from "@/lib/task-detect";
import { SessionStatus } from "@/lib/types";
import { SvelteMap, SvelteSet } from "svelte/reactivity";

/** Manifests whose edits change the known-task set. */
const MANIFESTS = ["package.json", "Cargo.toml", "Makefile", "pyproject.toml"];

/** Joins a task key's two parts. NUL can appear in neither a path nor a shell
 *  command, so the composed key can never collide with a real dir/command pair. */
const KEY_SEPARATOR = "\u0000";

/** Unique key for a task: its directory + command (matches the Tasks panel). */
export function taskKey({ dir, command }: {
  dir: string;
  command: string;
}): string {
  return `${dir}${KEY_SEPARATOR}${command}`;
}

/** Task keys currently reflected as running. */
const running = new SvelteSet<string>();
/** The task each session is currently running, so we can clear on idle/exit. */
const bySession = new SvelteMap<string, string>();

/** Whether a task (by key) is currently running (reactive). */
export function isTaskRunning(key: string): boolean {
  return running.has(key) || runnerRows().some(row =>
    !row.done && taskKey({
      dir: row.cwd,
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

/** The known task commands, refreshed from the backend on manifest changes. */
let commands: {
  key: string;
  command: string;
}[] = [];
// Set by App once per WebView. Keeping this as a getter means a long-lived
// listener always scans the project that window currently displays.
let currentProjectGetter: (() => string) | undefined;

function currentProject(): string {
  return currentProjectGetter?.() ?? "";
}

async function refreshCommands(): Promise<void> {
  const cwd = currentProject();
  if (!cwd) {
    commands = [];
    return;
  }

  try {
    const groups = await tasksApi.list(cwd);
    // A project switch can happen while the scan is in flight. A command from
    // the previous workspace must not be used to classify this window's PTY
    // output after it has moved on.
    if (cwd !== currentProject()) {
      return;
    }

    commands = groups.flatMap(group =>
      group.tasks.map(task => ({
        key: taskKey({
          dir: group.dir,
          command: task.command
        }),
        command: task.command
      })));
  } catch {
    if (cwd !== currentProject()) {
      return;
    }

    commands = [];
  }
}

function detect({ sessionId, chunk }: {
  sessionId: string;
  chunk: string;
}): void {
  const lines = chunk.split("\n");
  for (const { key, command } of commands) {
    if (lines.some(line => isTaskInvocation({
      line,
      command
    }))) {
      markSessionTask({
        sessionId,
        key
      });
      return;
    }
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

let started = false;

/** Start watching agent output for known-task runs. Idempotent; call once from
 *  the app shell (like the runner listeners). */
export async function initTaskRunDetection(project: () => string): Promise<void> {
  currentProjectGetter = project;

  if (started) {
    await refreshCommands();
    return;
  }

  started = true;

  await refreshCommands();
  await feed.onChange(async event => {
    if (MANIFESTS.includes(baseName(event.path))) {
      await refreshCommands();
    }
  });
  await pty.onData(chunk => detect({
    sessionId: chunk.id,
    chunk: chunk.data
  }));
  await pty.onExit(id => clearSessionTask(id));
}

/** Re-read task commands after this window switches projects. */
export async function refreshTaskRunDetection(): Promise<void> {
  await refreshCommands();
}
