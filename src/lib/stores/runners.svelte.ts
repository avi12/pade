// Task-runner dock state (SoC: shared state in lib/stores). A "runner" is a task
// launched to stream its output live into the dock instead of a throwaway
// terminal tab. This store owns the client-side runner list, subscribes once to
// the backend stream, and exposes start/stop/pipe. Piping a runner's output into
// an agent is done here via pty.write (the backend has no such command by design).

import { pty, runner } from "@/lib/bridge";
import type { TaskGroup } from "@/lib/types";

type RunnerKind = TaskGroup["kind"];

export interface RunnerRow {
  id: string;
  label: string;
  kind: RunnerKind;
  command: string;
  cwd: string;
  /** Captured stdout/stderr lines, in arrival order. */
  lines: string[];
  /** True once the process has exited. */
  done: boolean;
}

let rows = $state<RunnerRow[]>([]);
let listening = false;

/** The live runners (reactive). */
export function runnerRows(): RunnerRow[] {
  return rows;
}

/** Subscribe to the backend runner stream exactly once (call from App onMount). */
export async function ensureRunnerListeners(): Promise<void> {
  if (listening) {
    return;
  }

  listening = true;
  await runner.onData(({ id, data }) => {
    const row = rows.find(item => item.id === id);
    if (row) {
      row.lines.push(data);
    }
  });
  await runner.onExit(({ id }) => {
    const row = rows.find(item => item.id === id);
    if (row) {
      row.done = true;
    }
  });
}

/** Launch a task as a tracked runner streaming into the dock. */
export async function startRunner({ label, kind, command, cwd }: {
  label: string;
  kind: RunnerKind;
  command: string;
  cwd: string;
}): Promise<void> {
  const id = `run-${crypto.randomUUID()}`;
  rows.push({
    id,
    label,
    kind,
    command,
    cwd,
    lines: [],
    done: false
  });
  await runner.start({
    id,
    command,
    cwd
  });
}

/** Stop a runner and drop it from the dock. */
export async function stopRunner(id: string): Promise<void> {
  await runner.stop(id);
  rows = rows.filter(row => row.id !== id);
}

/** Pipe a runner's captured output into an agent session's input. */
export async function pipeRunner({ id, sessionId }: {
  id: string;
  sessionId: string;
}): Promise<void> {
  const row = rows.find(item => item.id === id);
  if (!row || !sessionId) {
    return;
  }

  await pty.write({
    id: sessionId,
    data: `${row.lines.join("\n")}\n`
  });
}
