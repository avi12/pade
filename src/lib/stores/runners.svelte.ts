// Task-runner dock state (SoC: shared state in lib/stores). A "runner" is a task
// launched to stream its output live into the dock instead of a throwaway
// terminal tab. This store owns the client-side runner list, subscribes once to
// the backend stream, and exposes start/stop/pipe. Piping a runner's output into
// an agent is done here via pty.write (the backend has no such command by design).

import { pty, runner } from "@/lib/bridge";
import { showToast } from "@/lib/stores/toast.svelte";
import { pastedText } from "@/lib/terminal-input";
import type { TaskGroup } from "@/lib/types";
import { RunnerStream } from "@/lib/types";

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
  /** The backend process id for the current run. A re-run spawns under a fresh
   * one so a stale exit event from the replaced process can never mark the new
   * run as done; the row's `id` stays stable for the UI. */
  backendId: string;
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
  /** Set when this row tracks a task the AGENT started (detected in its output),
   *  not one PADE spawned. Such a row has NO captured output — the task runs in the
   *  agent's own terminal, which PADE can't tee — but its Stop kills the real
   *  process tree, and a liveness poll drops the row when the process exits. */
  attached?: {
    sessionId: string;
  };
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
    const row = rows.find(item => item.backendId === id);
    if (!row) {
      return;
    }

    row.lines.push({
      text: data,
      stream
    });

    if (row.lines.length > MAX_LINES) {
      row.lines.splice(0, row.lines.length - MAX_LINES);
    }
  });
  await runner.onExit(({ id, code }) => {
    const row = rows.find(item => item.backendId === id);
    if (!row) {
      return;
    }

    row.done = true;
    row.failed = code !== null && code !== 0;
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
    backendId: id,
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

/** How often an attached runner checks whether the agent's task is still alive.
 *  A poll, not an event: a process the agent owns has no exit signal PADE hears. */
const ATTACHED_POLL_MS = 4000;
let attachedPoll: ReturnType<typeof setInterval> | undefined;

/** Track a task the agent started as an ATTACHED runner: a dock card with a Stop
 *  that kills the agent's process, but no captured output (PADE never owned it).
 *  Idempotent per (session, command) so repeated detections never stack rows.
 *  Returns the row id. */
export function attachRunner({ sessionId, label, kind, command, cwd }: {
  sessionId: string;
  label: string;
  kind: RunnerKind;
  command: string;
  cwd: string;
}): string {
  const existing = rows.find(row => row.attached?.sessionId === sessionId && row.command === command);
  if (existing) {
    return existing.id;
  }

  const id = `attached-${crypto.randomUUID()}`;
  rows.push({
    id,
    backendId: id,
    label,
    kind,
    command,
    cwd,
    lines: [],
    done: false,
    failed: false,
    attached: {
      sessionId
    }
  });
  ensureAttachedPoll();
  return id;
}

/** Start the liveness poll if any attached row needs it; stop it once none do, so
 *  the timer lives no longer than the rows it serves. */
function ensureAttachedPoll(): void {
  if (attachedPoll !== undefined || !rows.some(row => row.attached)) {
    return;
  }

  attachedPoll = setInterval(async () => {
    await reconcileAttachedRunners();
  }, ATTACHED_POLL_MS);
}

/** Drop each attached row whose task is no longer running — it finished, or its
 *  agent session exited (which kills the tree). Stops the poll when none remain. */
async function reconcileAttachedRunners(): Promise<void> {
  for (const row of rows.filter(item => item.attached)) {
    const sessionId = row.attached?.sessionId;
    if (sessionId === undefined) {
      continue;
    }

    const running = await pty.sessionTaskRunning({
      id: sessionId,
      command: row.command
    }).catch(() => false);
    if (!running) {
      rows = rows.filter(item => item.id !== row.id);
    }
  }

  if (!rows.some(row => row.attached) && attachedPoll !== undefined) {
    clearInterval(attachedPoll);
    attachedPoll = undefined;
  }
}

/** The re-run divider, composed from named parts so the value reads on its own. */
const RERUN_RULE = "─".repeat(3);

function rerunDivider(): string {
  const time = new Date().toLocaleTimeString([], {
    hour: "2-digit",
    minute: "2-digit",
    second: "2-digit"
  });
  return `${RERUN_RULE} re-run · ${time} ${RERUN_RULE}`;
}

/** Re-run a runner's command: kill the current process (if still live) and spawn
 * it afresh under a new backend id. `preserve` keeps the captured output and
 * appends a timestamped divider before the new run's lines; otherwise the pane
 * starts clean. */
export async function rerunRunner({ id, preserve }: {
  id: string;
  preserve: boolean;
}): Promise<void> {
  const row = rows.find(item => item.id === id);
  if (!row) {
    return;
  }

  const previousBackendId = row.backendId;
  if (preserve) {
    row.lines.push(
      {
        text: "",
        stream: RunnerStream.enum.stdout
      },
      {
        text: rerunDivider(),
        stream: RunnerStream.enum.stdout
      }
    );
  } else {
    row.lines = [];
  }

  row.done = false;
  row.failed = false;
  row.backendId = `run-${crypto.randomUUID()}`;
  try {
    await runner.stop(previousBackendId);
    await runner.start({
      id: row.backendId,
      command: row.command,
      cwd: row.cwd
    });
  } catch (error) {
    // Surface the restart failure in the pane itself: crit dot + the error text.
    row.done = true;
    row.failed = true;
    row.lines.push({
      text: String(error),
      stream: RunnerStream.enum.stderr
    });
    return;
  }
  showToast(
    preserve
      ? `Re-running “${row.label}” · kept previous output`
      : `Re-running “${row.label}”`
  );
}

/** Stop a runner and drop it from the dock. An attached runner isn't ours to
 *  stop, so kill the agent's task process tree instead of a PADE-spawned one. */
export async function stopRunner(id: string): Promise<void> {
  const row = rows.find(item => item.id === id);
  if (!row) {
    return;
  }

  if (row.attached) {
    await pty.sessionTaskStop({
      id: row.attached.sessionId,
      command: row.command
    });
  } else {
    await runner.stop(row.backendId);
  }

  rows = rows.filter(item => item.id !== id);
}

/** Stop and forget every runner belonging to this window. Workspace changes
 * must not leave a previous project's dev server running or keep its dock open
 * over the next project. Every stop is attempted even if one backend call
 * fails; rows are then cleared so late output/exit events are ignored. */
export async function stopAllRunners(): Promise<void> {
  const backendIds = rows.map(row => row.backendId);
  await Promise.all(backendIds.map(backendId => runner.stop(backendId).catch(() => {})));
  rows = [];
}

/** Commit a drag-reordered runner order (the drag engine hands us the full id
 * order on drop). Ids it doesn't know keep their rows appended in place. */
export function setRunnerOrder(orderedIds: string[]): void {
  const reordered = orderedIds.flatMap(id => {
    const row = rows.find(item => item.id === id);
    return row ? [row] : [];
  });
  const leftover = rows.filter(row => !orderedIds.includes(row.id));
  rows = [...reordered, ...leftover];
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
    data: pastedText(row.lines.map(line => line.text).join("\n"))
  });
}
