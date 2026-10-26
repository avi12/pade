import { Command, EventType } from "@/lib/ipc/messaging";
import { projectRunnerRows, runnerRows, stopRunner } from "@/lib/stores/tasks/runners.svelte";
import { taskCatalog } from "@/lib/stores/tasks/taskCatalog.svelte";
import { adoptRunningTasks, initTaskRunDetection, isTaskRunning, taskKey } from "@/lib/stores/tasks/taskRuns.svelte";
import { setFrontProjectSource } from "@/lib/stores/workspace/frontProject.svelte";
import type { RunnerInfo } from "@/lib/types";
import {
  beforeEach,
  describe,
  expect,
  it,
  type Mock,
  vi
} from "vitest";

type PtyChunkHandler = (chunk: {
  id: string;
  data: string;
}) => void;

type MessageHandler = (message: { data: unknown }) => void;

const mocks = vi.hoisted<{
  onData: PtyChunkHandler | undefined;
  taskRunning: boolean;
  runningChecks: number;
  backendRunners: RunnerInfo[];
  sendMessage: Mock<(command: Command, data?: unknown) => Promise<unknown>>;
  onMessage: Mock<(type: EventType, handler: MessageHandler) => Promise<() => void>>;
}>(() => ({
  onData: undefined,
  taskRunning: true,
  runningChecks: 0,
  backendRunners: [],
  sendMessage: vi.fn(),
  onMessage: vi.fn()
}));

vi.mock("@/lib/ipc/messaging", async importOriginal => ({
  ...await importOriginal<typeof import("@/lib/ipc/messaging")>(),
  sendMessage: mocks.sendMessage,
  onMessage: mocks.onMessage
}));

const DEMO_TASKS = [{
  manifest: "Cargo.toml",
  dir: "demo",
  kind: "cargo",
  tasks: [{
    name: "check",
    command: "cargo check"
  }]
}];

mocks.sendMessage.mockImplementation(async command => {
  if (command === Command.sessionTaskRunning) {
    mocks.runningChecks += 1;
    return mocks.taskRunning;
  }

  if (command === Command.sessionTaskStop) {
    return true;
  }

  if (command === Command.tasksDescriptors) {
    return [];
  }

  if (command === Command.tasksList) {
    return DEMO_TASKS;
  }

  if (command === Command.runnerList) {
    return mocks.backendRunners;
  }

  return undefined;
});

mocks.onMessage.mockImplementation(async (type, handler) => {
  if (type === EventType.ptyData) {
    mocks.onData = chunk => handler({ data: chunk });
  }

  return () => undefined;
});

/** Let the detector's liveness check and its attach settle - it is fired from a
 *  synchronous PTY handler, so the row can only appear on a later turn of the
 *  event loop. Used where the assertion is that NOTHING attached; a positive
 *  assertion waits for the row instead. */
async function settle() {
  await new Promise(resolve => setTimeout(resolve, 0));
}

describe("task run detection", () => {
  // `initTaskRunDetection` subscribes once for the module's lifetime, so the
  // captured handler is deliberately kept across tests; only the rows and the
  // mocked answers reset.
  beforeEach(async () => {
    setFrontProjectSource(() => "demo");
    mocks.taskRunning = true;
    mocks.runningChecks = 0;
    for (const row of runnerRows()) {
      await stopRunner(row.id);
    }
  });

  it("attaches once when a fullscreen TUI repaints one invocation", async () => {
    await initTaskRunDetection();

    const chunk = {
      id: "session-1",
      data: "Bash(cargo check 2>&1 | tail -40)\n"
    };
    mocks.onData?.(chunk);
    await vi.waitFor(() => expect(runnerRows()).toHaveLength(1));

    mocks.onData?.(chunk);
    await settle();

    expect(runnerRows()).toHaveLength(1);
    expect(mocks.runningChecks).toBe(1);
    expect(mocks.sendMessage).toHaveBeenCalledWith(Command.sessionTaskRunning, {
      id: "session-1",
      command: "cargo check"
    });
  });

  it("ignores a repainted invocation whose process is long gone", async () => {
    await initTaskRunDetection();
    mocks.taskRunning = false;

    mocks.onData?.({
      id: "session-gone",
      data: "Bash(cargo check)\n"
    });
    await settle();

    // The line stays on a fullscreen agent's screen forever. Attaching on the
    // text alone made the card appear here, the liveness poll drop it, and the
    // next repaint bring it back - flickering the dock and resizing the terminal.
    expect(runnerRows()).toHaveLength(0);
  });

  it("stops reporting a task as running the moment its row goes", async () => {
    await initTaskRunDetection();
    const key = taskKey({
      directory: "demo",
      command: "cargo check"
    });

    mocks.onData?.({
      id: "session-2",
      data: "Bash(cargo check)\n"
    });
    await vi.waitFor(() => expect(isTaskRunning(key)).toBe(true));

    // The row is what tracks the process - the liveness poll drops it when the
    // process exits, and `stopRunner` drops it on demand. Either way the panel
    // has to follow it, and NOT hold the badge until the agent's turn ends.
    for (const row of runnerRows()) {
      await stopRunner(row.id);
    }

    expect(isTaskRunning(key)).toBe(false);
  });

  it("files a detected task under the project whose catalog listed it", async () => {
    await initTaskRunDetection();

    mocks.onData?.({
      id: "session-3",
      data: "Bash(cargo check)\n"
    });
    await vi.waitFor(() => expect(projectRunnerRows("demo")).toHaveLength(1));

    // Another project in front sees none of it, and its dock stays hidden.
    expect(projectRunnerRows("other")).toHaveLength(0);
  });

  it("brings back the card of a task still running once its project lists it", async () => {
    await initTaskRunDetection();
    mocks.backendRunners = [{
      id: "run-left-over",
      command: "cargo check",
      cwd: "demo",
      startedAt: 0
    }, {
      id: "run-elsewhere",
      command: "cargo check",
      cwd: "other",
      startedAt: 0
    }];

    await adoptRunningTasks(taskCatalog.snapshot);
    await adoptRunningTasks(taskCatalog.snapshot);

    // Only the task the catalog in front lists, and only once.
    expect(projectRunnerRows("demo").map(row => row.backendId)).toEqual(["run-left-over"]);
    mocks.backendRunners = [];
  });
});
