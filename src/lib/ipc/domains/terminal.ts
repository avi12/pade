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
import { z } from "zod";

/** A task the agent started itself, addressed by its session and command line. */
type SessionTask = {
  id: string;
  command: string;
};

export const TERMINAL_COMMANDS = {
  ptySpawn: "pty_spawn",
  ptyWrite: "pty_write",
  ptyResize: "pty_resize",
  ptyKill: "pty_kill",
  ptyList: "pty_list",
  ptyHistory: "pty_history",
  sessionTaskStop: "session_task_stop",
  sessionTaskRunning: "session_task_running",
  runnerStart: "runner_start",
  runnerStop: "runner_stop",
  runnerList: "runner_list",
  tasksDescriptors: "tasks_descriptors",
  tasksList: "tasks_list"
} as const;

export const TERMINAL_EVENTS = {
  ptyData: "pty://data",
  ptyExit: "pty://exit",
  runnerData: "runner://data",
  runnerExit: "runner://exit"
} as const;

export interface TerminalCommandProtocol {
  /** `args`: extra args for `command`. `scheme`: the resolved appearance, so
   *  env-themed CLIs start matching it. `conversationId`: the conversation to
   *  pin/resume (`claude --session-id`). `modelChoice`: launch flags for this
   *  session alone, never the agent's saved default. */
  [TERMINAL_COMMANDS.ptySpawn]: (data: {
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
  [TERMINAL_COMMANDS.ptyWrite]: (data: {
    id: string;
    data: string;
  }) => void;
  [TERMINAL_COMMANDS.ptyResize]: (data: {
    id: string;
    cols: number;
    rows: number;
  }) => void;
  [TERMINAL_COMMANDS.ptyKill]: (data: { id: string }) => void;
  /** Every live session the backend still hosts - the roster a reloaded window
   *  re-attaches from (see `session-restore`). */
  [TERMINAL_COMMANDS.ptyList]: () => PtySession[];
  /** What a terminal attaching mid-flight needs to paint, and the sequence
   *  number of the last chunk already inside it. */
  [TERMINAL_COMMANDS.ptyHistory]: (data: { id: string }) => PtyHistory;
  /** Stop a task the agent started itself; true when a matching process was found. */
  [TERMINAL_COMMANDS.sessionTaskStop]: (data: SessionTask) => boolean;
  /** Whether a task the agent started is still running under its tree. */
  [TERMINAL_COMMANDS.sessionTaskRunning]: (data: SessionTask) => boolean;
  /** Launch a task as a tracked runner (the dock) that streams its output. */
  [TERMINAL_COMMANDS.runnerStart]: (data: {
    id: string;
    command: string;
    cwd?: string | null;
  }) => void;
  [TERMINAL_COMMANDS.runnerStop]: (data: { id: string }) => void;
  [TERMINAL_COMMANDS.runnerList]: () => RunnerInfo[];
  [TERMINAL_COMMANDS.tasksDescriptors]: () => TaskManifestDescriptor[];
  /** Runnable tasks parsed from the project's manifests. */
  [TERMINAL_COMMANDS.tasksList]: (data: { cwd: string }) => TaskGroup[];
}

export const TERMINAL_RESPONSE_SCHEMAS = {
  [TERMINAL_COMMANDS.ptySpawn]: NO_REPLY,
  [TERMINAL_COMMANDS.ptyWrite]: NO_REPLY,
  [TERMINAL_COMMANDS.ptyResize]: NO_REPLY,
  [TERMINAL_COMMANDS.ptyKill]: NO_REPLY,
  [TERMINAL_COMMANDS.ptyList]: z.array(PtySession),
  [TERMINAL_COMMANDS.ptyHistory]: PtyHistory,
  [TERMINAL_COMMANDS.sessionTaskStop]: z.boolean(),
  [TERMINAL_COMMANDS.sessionTaskRunning]: z.boolean(),
  [TERMINAL_COMMANDS.runnerStart]: NO_REPLY,
  [TERMINAL_COMMANDS.runnerStop]: NO_REPLY,
  [TERMINAL_COMMANDS.runnerList]: z.array(RunnerInfo),
  [TERMINAL_COMMANDS.tasksDescriptors]: z.array(TaskManifestDescriptor),
  [TERMINAL_COMMANDS.tasksList]: z.array(TaskGroup)
} satisfies { [Key in keyof TerminalCommandProtocol]: z.ZodType<ReturnType<TerminalCommandProtocol[Key]>> };

export interface TerminalEventProtocol {
  [TERMINAL_EVENTS.ptyData]: (data: PtyChunk) => void;
  [TERMINAL_EVENTS.ptyExit]: (data: PtyExit) => void;
  [TERMINAL_EVENTS.runnerData]: (data: RunnerData) => void;
  [TERMINAL_EVENTS.runnerExit]: (data: RunnerExit) => void;
}

export const TERMINAL_PAYLOAD_SCHEMAS = {
  [TERMINAL_EVENTS.ptyData]: PtyChunk,
  [TERMINAL_EVENTS.ptyExit]: PtyExit,
  [TERMINAL_EVENTS.runnerData]: RunnerData,
  [TERMINAL_EVENTS.runnerExit]: RunnerExit
} satisfies { [Key in keyof TerminalEventProtocol]: z.ZodType<Parameters<TerminalEventProtocol[Key]>[0]> };
