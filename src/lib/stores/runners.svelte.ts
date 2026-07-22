// Task-runner dock state (SoC: shared state in lib/stores). A "runner" is a task
// launched to stream its output live into the dock instead of a throwaway
// terminal tab. This store owns the client-side runner list, subscribes once to
// the backend stream, and exposes start/stop/pipe. Piping a runner's output into
// an agent is done here via pty.write (the backend has no such command by design).

import { pty, runner } from "@/lib/bridge";
import type { RunnerStream, TaskGroup } from "@/lib/types";

type RunnerKind = TaskGroup["kind"];

/**
 * Cap on captured lines per runner. Runners are dev servers / watchers that
 * stream forever, so an uncapped buffer blows up memory and the DOM. Once a row
 * exceeds this, the oldest lines are dropped from the head.
 */
const MAX_LINES = 5000;

/** One captured output line plus which stream it arrived on (for stderr tinting). */
export interface RunnerLine {
  text: string;
  stream: RunnerStream;
}

export interface RunnerRow {
  id: string;
  label: string;
  kind: RunnerKind;
  command: string;
  cwd: string;
  /** Captured stdout/stderr lines, in arrival order. */
  lines: RunnerLine[];
  /** True once the process has exited. */
  done: boolean;
  /** True once the process exited with a non-zero code (failure). */
  failed: boolean;
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
  await runner.onData(({ id, data, stream }) => {
    const row = rows.find(item => item.id === id);
    if (row) {
      row.lines.push({
        text: data,
        stream
      });

      if (row.lines.length > MAX_LINES) {
        row.lines.splice(0, row.lines.length - MAX_LINES);
      }
    }
  });
  await runner.onExit(({ id, code }) => {
    const row = rows.find(item => item.id === id);
    if (row) {
      row.done = true;
      row.failed = code !== null && code !== 0;
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
    done: false,
    failed: false
  });
  try {
    await runner.start({
      id,
      command,
      cwd
    });
  } catch (error) {
    rows = rows.filter(row => row.id !== id);
    throw error;
  }
}

/** Stop a runner and drop it from the dock. */
export async function stopRunner(id: string): Promise<void> {
  await runner.stop(id);
  rows = rows.filter(row => row.id !== id);
}

/** Stop and forget every runner belonging to this window. Workspace changes
 * must not leave a previous project's dev server running or keep its dock open
 * over the next project. Every stop is attempted even if one backend call
 * fails; rows are then cleared so late output/exit events are ignored. */
export async function stopAllRunners(): Promise<void> {
  const ids = rows.map(row => row.id);
  await Promise.all(ids.map(id => runner.stop(id).catch(() => {})));
  rows = [];
}

/** Move a runner so it sits just before `beforeId` — pointer drag-to-reorder. */
export function moveRunnerBefore({ id, beforeId }: {
  id: string;
  beforeId: string;
}): void {
  if (id === beforeId) {
    return;
  }

  const from = rows.findIndex(row => row.id === id);
  if (from === -1 || rows.findIndex(row => row.id === beforeId) === -1) {
    return;
  }

  const [moved] = rows.splice(from, 1);
  const insertAt = rows.findIndex(row => row.id === beforeId);
  rows.splice(insertAt, 0, moved);
}

/** Nudge a runner one slot earlier or later — keyboard reorder. */
export function moveRunnerBy({ id, delta }: {
  id: string;
  delta: number;
}): void {
  const from = rows.findIndex(row => row.id === id);
  if (from === -1) {
    return;
  }

  const to = Math.min(Math.max(from + delta, 0), rows.length - 1);
  if (to === from) {
    return;
  }

  const [moved] = rows.splice(from, 1);
  rows.splice(to, 0, moved);
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
    data: `${row.lines.map(line => line.text).join("\n")}\n`
  });
}
