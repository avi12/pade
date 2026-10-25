// Task-runner dock state (SoC: shared state in lib/stores). A "runner" is a task
// launched to stream its output live into the dock instead of a throwaway
// terminal tab. This store owns the client-side runner list, subscribes once to
// the backend stream, and exposes start/stop/pipe. Piping a runner's output into
// an agent is done here via pty_write (the backend has no such command by design).

import { Command, EventType, onMessage, sendMessage } from "@/lib/ipc/messaging";
import { showToast } from "@/lib/stores/ui/toast.svelte";
import { pastedText } from "@/lib/terminal/input/terminal-input";
import type { TaskGroup } from "@/lib/types";
import { RunnerStream } from "@/lib/types";
import { settledValue } from "@/lib/util/async/settled";

type RunnerKind = TaskGroup["kind"];

/**
 * Cap on captured lines per runner. Runners are dev servers / watchers that
 * stream forever, so an uncapped buffer blows up memory and the DOM. Once a row
 * exceeds this, the oldest lines are dropped from the head.
 */
const MAX_LINES = 5000;

/** One captured output line plus which stream it arrived on (for stderr tinting). */
interface RunnerLine {
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
  /** The project the task belongs to. The dock shows only the runners of the
   *  project in front; the others keep running and come back with it. */
  project: string;
  /** Captured stdout/stderr lines, in arrival order. */
  lines: RunnerLine[];
  /** True once the process has exited. */
  isDone: boolean;
  /** True once the process exited with a non-zero code (failure). */
  isFailed: boolean;
  /** Set when this row tracks a task the AGENT started (detected in its output),
   *  not one PADE spawned. Such a row has NO captured output - the task runs in the
   *  agent's own terminal, which PADE can't tee - but its Stop kills the real
   *  process tree, and a liveness poll drops the row when the process exits. */
  attached?: {
    sessionId: string;
  };
}

/** A task as the dock runs it: what it is and where, under its project. */
export type RunnerTask = Pick<RunnerRow, "label" | "kind" | "command" | "cwd" | "project">;

/** Id prefixes: a process PADE spawned, and a task the agent started. */
const SPAWNED_ID_PREFIX = "run";
const ATTACHED_ID_PREFIX = "attached";
const SUCCESS_EXIT_CODE = 0;

let rows = $state<RunnerRow[]>([]);
let isListening = false;

/** The live runners (reactive). */
export function runnerRows() {
  return rows;
}

/** The live runners of one project (reactive) - what its dock shows. */
export function projectRunnerRows(project: string) {
  return rows.filter(row => row.project === project);
}

function newRunnerId(prefix: string) {
  return `${prefix}-${crypto.randomUUID()}`;
}

function rowById(id: string) {
  return rows.find(row => row.id === id);
}

function rowByBackendId(backendId: string) {
  return rows.find(row => row.backendId === backendId);
}

function removeRow(id: string) {
  rows = rows.filter(row => row.id !== id);
}

/** Add a fresh, still-running row for a task; its backend id starts as its id. */
function addRow(row: RunnerTask & Pick<RunnerRow, "id" | "attached">) {
  rows.push({
    ...row,
    backendId: row.id,
    lines: [],
    isDone: false,
    isFailed: false
  });
}

/** Subscribe to the backend runner stream exactly once (call from App onMount). */
export async function ensureRunnerListeners() {
  if (isListening) {
    return;
  }

  isListening = true;
  await onMessage(EventType.runnerData, ({ data: { id, data, stream } }) => {
    const row = rowByBackendId(id);
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
  await onMessage(EventType.runnerExit, ({ data: { id, code } }) => {
    const row = rowByBackendId(id);
    if (!row) {
      return;
    }

    row.isDone = true;
    row.isFailed = code !== null && code !== SUCCESS_EXIT_CODE;
  });
}

/** Launch a task as a tracked runner streaming into the dock. */
export async function startRunner({ label, kind, command, cwd, project }: RunnerTask) {
  const id = newRunnerId(SPAWNED_ID_PREFIX);
  addRow({
    id,
    label,
    kind,
    command,
    cwd,
    project
  });
  try {
    await sendMessage(Command.runnerStart, {
      id,
      command,
      cwd
    });
  } catch (error) {
    removeRow(id);
    throw error;
  }
}

/** How often PADE asks the OS whether a task the agent started is still alive.
 *  A poll, not an event: a process the agent owns has no exit signal PADE hears.
 *  The one cadence for that question - `taskRuns` throttles its own checks to it
 *  too, so a repainted invocation line can never ask the process tree faster. */
export const TASK_LIVENESS_POLL_MS = 4000;
let attachedPoll: ReturnType<typeof setInterval> | undefined;

/** The attached row already tracking `command` for this session, if any. One
 *  lookup serving both the idempotent attach below and `taskRuns`, which skips
 *  asking the OS about a task the dock is already showing. */
export function attachedRunner({ sessionId, command }: {
  sessionId: string;
  command: string;
}) {
  return rows.find(row => row.attached?.sessionId === sessionId && row.command === command);
}

/** Track a task the agent started as an ATTACHED runner: a dock card with a Stop
 *  that kills the agent's process, but no captured output (PADE never owned it).
 *  Idempotent per (session, command) so repeated detections never stack rows.
 *  Returns the row id. */
export function attachRunner({ sessionId, label, kind, command, cwd, project }: RunnerTask & {
  sessionId: string;
}) {
  const existing = attachedRunner({
    sessionId,
    command
  });
  if (existing) {
    return existing.id;
  }

  const id = newRunnerId(ATTACHED_ID_PREFIX);
  addRow({
    id,
    label,
    kind,
    command,
    cwd,
    project,
    attached: {
      sessionId
    }
  });
  ensureAttachedPoll();
  return id;
}

function hasAttachedRows() {
  return rows.some(row => row.attached);
}

/** Start the liveness poll if any attached row needs it; stop it once none do, so
 *  the timer lives no longer than the rows it serves. */
function ensureAttachedPoll() {
  if (attachedPoll !== undefined || !hasAttachedRows()) {
    return;
  }

  attachedPoll = setInterval(async () => {
    await reconcileAttachedRunners();
  }, TASK_LIVENESS_POLL_MS);
}

/** Drop each attached row whose task is no longer running - it finished, or its
 *  agent session exited (which kills the tree). Stops the poll when none remain. */
async function reconcileAttachedRunners() {
  for (const row of rows.filter(item => item.attached)) {
    const sessionId = row.attached?.sessionId;
    if (sessionId === undefined) {
      continue;
    }

    const isRunning = await settledValue({
      promise: sendMessage(Command.sessionTaskRunning, {
        id: sessionId,
        command: row.command
      }),
      fallback: false
    });
    if (!isRunning) {
      removeRow(row.id);
    }
  }

  if (!hasAttachedRows() && attachedPoll !== undefined) {
    clearInterval(attachedPoll);
    attachedPoll = undefined;
  }
}

/** The re-run divider, composed from named parts so the value reads on its own. */
const RERUN_RULE = "─".repeat(3);

function rerunDivider() {
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
}) {
  const row = rowById(id);
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

  row.isDone = false;
  row.isFailed = false;
  row.backendId = newRunnerId(SPAWNED_ID_PREFIX);
  try {
    await sendMessage(Command.runnerStop, { id: previousBackendId });
    await sendMessage(Command.runnerStart, {
      id: row.backendId,
      command: row.command,
      cwd: row.cwd
    });
  } catch (error) {
    // Surface the restart failure in the pane itself: crit dot + the error text.
    row.isDone = true;
    row.isFailed = true;
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
export async function stopRunner(id: string) {
  const row = rowById(id);
  if (!row) {
    return;
  }

  if (row.attached) {
    await sendMessage(Command.sessionTaskStop, {
      id: row.attached.sessionId,
      command: row.command
    });
  } else {
    await sendMessage(Command.runnerStop, { id: row.backendId });
  }

  removeRow(id);
}

/** Stop and forget every runner belonging to this window. Workspace changes
 * must not leave a previous project's dev server running or keep its dock open
 * over the next project. Every stop is attempted even if one backend call
 * fails; rows are then cleared so late output/exit events are ignored. */
export async function stopAllRunners() {
  await Promise.allSettled(rows.map(row => sendMessage(Command.runnerStop, { id: row.backendId })));
  rows = [];
}

/** Commit a drag-reordered runner order (the drag engine hands us the full id
 * order on drop). Ids it doesn't know keep their rows appended in place. */
export function setRunnerOrder(orderedIds: string[]) {
  const reordered = orderedIds.flatMap(id => {
    const row = rowById(id);
    return row ? [row] : [];
  });
  const leftover = rows.filter(row => !orderedIds.includes(row.id));
  rows = [...reordered, ...leftover];
}

/** Pipe a runner's captured output into an agent session's input. */
export async function pipeRunner({ id, sessionId }: {
  id: string;
  sessionId: string;
}) {
  const row = rowById(id);
  if (!row || !sessionId) {
    return;
  }

  await sendMessage(Command.ptyWrite, {
    id: sessionId,
    data: pastedText(row.lines.map(line => line.text).join("\n"))
  });
}
