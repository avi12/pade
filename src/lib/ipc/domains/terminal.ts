import { z } from "zod";

import { NO_REPLY } from "@/lib/ipc/domains/no-reply";
import type { ModelChoice } from "@/lib/types/agent";
import {
  PtyChunk,
  PtyExit,
  PtyHistory,
  PtySession,
  RunnerData,
  RunnerExit,
  RunnerInfo,
  TaskGroup,
  TaskManifestDescriptor
} from "@/lib/types/terminal";
import type { Scheme } from "@/lib/types/theme";

/** A task the agent started itself, addressed by its session and command line. */
interface SessionTask {
  id: string;
  command: string;
}

export const TERMINAL_COMMANDS = {
  PTY_SPAWN: "pty_spawn",
  PTY_WRITE: "pty_write",
  PTY_RESIZE: "pty_resize",
  PTY_KILL: "pty_kill",
  PTY_LIST: "pty_list",
  PTY_HISTORY: "pty_history",
  SESSION_TASK_STOP: "session_task_stop",
  SESSION_TASK_RUNNING: "session_task_running",
  RUNNER_START: "runner_start",
  RUNNER_STOP: "runner_stop",
  RUNNER_LIST: "runner_list",
  TASKS_DESCRIPTORS: "tasks_descriptors",
  TASKS_LIST: "tasks_list"
} as const;

export const TERMINAL_EVENTS = {
  PTY_DATA: "pty://data",
  PTY_EXIT: "pty://exit",
  RUNNER_DATA: "runner://data",
  RUNNER_EXIT: "runner://exit"
} as const;

export interface TerminalCommandProtocol {
  /** `args`: extra args for `command`. `scheme`: the resolved appearance, so
   *  env-themed CLIs start matching it. `conversationId`: the conversation to
   *  pin/resume (`claude --session-id`). `modelChoice`: launch flags for this
   *  session alone, never the agent's saved default. */
  [TERMINAL_COMMANDS.PTY_SPAWN]: (data: {
    id: string;
    command: string | null;
    cwd: string | null;
    cols: number;
    rows: number;
    args?: string[];
    scheme?: Scheme;
    conversationId?: string;
    modelChoice?: ModelChoice;
  }) => void;
  [TERMINAL_COMMANDS.PTY_WRITE]: (data: { id: string; data: string }) => void;
  [TERMINAL_COMMANDS.PTY_RESIZE]: (data: { id: string; cols: number; rows: number }) => void;
  [TERMINAL_COMMANDS.PTY_KILL]: (data: { id: string }) => void;
  /** Every live session the backend still hosts - the roster a reloaded window
   *  re-attaches from (see `session-restore`). */
  [TERMINAL_COMMANDS.PTY_LIST]: () => PtySession[];
  /** What a terminal attaching mid-flight needs to paint, and the sequence
   *  number of the last chunk already inside it. */
  [TERMINAL_COMMANDS.PTY_HISTORY]: (data: { id: string }) => PtyHistory;
  /** Stop a task the agent started itself; true when a matching process was found. */
  [TERMINAL_COMMANDS.SESSION_TASK_STOP]: (data: SessionTask) => boolean;
  /** Whether a task the agent started is still running under its tree. */
  [TERMINAL_COMMANDS.SESSION_TASK_RUNNING]: (data: SessionTask) => boolean;
  /** Launch a task as a tracked runner (the dock) that streams its output. */
  [TERMINAL_COMMANDS.RUNNER_START]: (data: { id: string; command: string; cwd?: string | null }) => void;
  [TERMINAL_COMMANDS.RUNNER_STOP]: (data: { id: string }) => void;
  [TERMINAL_COMMANDS.RUNNER_LIST]: () => RunnerInfo[];
  [TERMINAL_COMMANDS.TASKS_DESCRIPTORS]: () => TaskManifestDescriptor[];
  /** Runnable tasks parsed from the project's manifests. */
  [TERMINAL_COMMANDS.TASKS_LIST]: (data: { cwd: string }) => TaskGroup[];
}

export const TERMINAL_RESPONSE_SCHEMAS = {
  [TERMINAL_COMMANDS.PTY_SPAWN]: NO_REPLY,
  [TERMINAL_COMMANDS.PTY_WRITE]: NO_REPLY,
  [TERMINAL_COMMANDS.PTY_RESIZE]: NO_REPLY,
  [TERMINAL_COMMANDS.PTY_KILL]: NO_REPLY,
  [TERMINAL_COMMANDS.PTY_LIST]: z.array(PtySession),
  [TERMINAL_COMMANDS.PTY_HISTORY]: PtyHistory,
  [TERMINAL_COMMANDS.SESSION_TASK_STOP]: z.boolean(),
  [TERMINAL_COMMANDS.SESSION_TASK_RUNNING]: z.boolean(),
  [TERMINAL_COMMANDS.RUNNER_START]: NO_REPLY,
  [TERMINAL_COMMANDS.RUNNER_STOP]: NO_REPLY,
  [TERMINAL_COMMANDS.RUNNER_LIST]: z.array(RunnerInfo),
  [TERMINAL_COMMANDS.TASKS_DESCRIPTORS]: z.array(TaskManifestDescriptor),
  [TERMINAL_COMMANDS.TASKS_LIST]: z.array(TaskGroup)
} satisfies { [Key in keyof TerminalCommandProtocol]: z.ZodType<ReturnType<TerminalCommandProtocol[Key]>> };

export interface TerminalEventProtocol {
  [TERMINAL_EVENTS.PTY_DATA]: (data: PtyChunk) => void;
  [TERMINAL_EVENTS.PTY_EXIT]: (data: PtyExit) => void;
  [TERMINAL_EVENTS.RUNNER_DATA]: (data: RunnerData) => void;
  [TERMINAL_EVENTS.RUNNER_EXIT]: (data: RunnerExit) => void;
}

export const TERMINAL_PAYLOAD_SCHEMAS = {
  [TERMINAL_EVENTS.PTY_DATA]: PtyChunk,
  [TERMINAL_EVENTS.PTY_EXIT]: PtyExit,
  [TERMINAL_EVENTS.RUNNER_DATA]: RunnerData,
  [TERMINAL_EVENTS.RUNNER_EXIT]: RunnerExit
} satisfies { [Key in keyof TerminalEventProtocol]: z.ZodType<Parameters<TerminalEventProtocol[Key]>[0]> };
