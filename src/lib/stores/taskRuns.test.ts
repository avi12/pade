import { runnerRows, stopRunner } from "@/lib/stores/runners.svelte";
import { initTaskRunDetection, isTaskRunning, taskKey } from "@/lib/stores/taskRuns.svelte";
import {
  beforeEach,
  describe,
  expect,
  it,
  vi
} from "vitest";

const mocks = vi.hoisted<{
  onData: ((chunk: {
    id: string;
    data: string;
  }) => void) | undefined;
  taskRunning: boolean;
  runningChecks: number;
}>(() => ({
  onData: undefined,
  taskRunning: true,
  runningChecks: 0
}));

vi.mock("@/lib/bridge", () => ({
  feed: {
    onChange: async () => () => undefined
  },
  pty: {
    async onData(callback: typeof mocks.onData) {
      mocks.onData = callback;
      return () => undefined;
    },
    onExit: async () => () => undefined,
    async sessionTaskRunning() {
      mocks.runningChecks += 1;
      return mocks.taskRunning;
    },
    sessionTaskStop: async () => true
  },
  tasks: {
    descriptors: async () => [],
    list: async () => [{
      manifest: "Cargo.toml",
      dir: "demo",
      kind: "cargo",
      tasks: [{
        name: "check",
        command: "cargo check"
      }]
    }]
  }
}));

/** Let the detector's liveness check and its attach settle — it is fired from a
 *  synchronous PTY handler, so the row can only appear on a later turn of the
 *  event loop. Used where the assertion is that NOTHING attached; a positive
 *  assertion waits for the row instead. */
async function settle(): Promise<void> {
  await new Promise(resolve => setTimeout(resolve, 0));
}

describe("task run detection", () => {
  // `initTaskRunDetection` subscribes once for the module's lifetime, so the
  // captured handler is deliberately kept across tests; only the rows and the
  // mocked answers reset.
  beforeEach(async () => {
    mocks.taskRunning = true;
    mocks.runningChecks = 0;
    for (const row of runnerRows()) {
      await stopRunner(row.id);
    }
  });

  it("attaches once when a fullscreen TUI repaints one invocation", async () => {
    await initTaskRunDetection(() => "demo");

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
  });

  it("ignores a repainted invocation whose process is long gone", async () => {
    await initTaskRunDetection(() => "demo");
    mocks.taskRunning = false;

    mocks.onData?.({
      id: "session-gone",
      data: "Bash(cargo check)\n"
    });
    await settle();

    // The line stays on a fullscreen agent's screen forever. Attaching on the
    // text alone made the card appear here, the liveness poll drop it, and the
    // next repaint bring it back — flickering the dock and resizing the terminal.
    expect(runnerRows()).toHaveLength(0);
  });

  it("stops reporting a task as running the moment its row goes", async () => {
    await initTaskRunDetection(() => "demo");
    const key = taskKey({
      directory: "demo",
      command: "cargo check"
    });

    mocks.onData?.({
      id: "session-2",
      data: "Bash(cargo check)\n"
    });
    await vi.waitFor(() => expect(isTaskRunning(key)).toBe(true));

    // The row is what tracks the process — the liveness poll drops it when the
    // process exits, and `stopRunner` drops it on demand. Either way the panel
    // has to follow it, and NOT hold the badge until the agent's turn ends.
    for (const row of runnerRows()) {
      await stopRunner(row.id);
    }

    expect(isTaskRunning(key)).toBe(false);
  });
});
