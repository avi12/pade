import { runnerRows, stopRunner } from "@/lib/stores/runners.svelte";
import { initTaskRunDetection, isTaskRunning, taskKey } from "@/lib/stores/taskRuns.svelte";
import { SessionStatus } from "@/lib/types";
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
}>(() => ({
  onData: undefined
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
    sessionTaskRunning: async () => true,
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

vi.mock("@/lib/stores/sessions.svelte", () => ({
  sessionStatus: () => SessionStatus.enum.working
}));

describe("task run detection", () => {
  beforeEach(() => {
    mocks.onData = undefined;
  });

  it("attaches once when a fullscreen TUI repaints one invocation", async () => {
    await initTaskRunDetection(() => "demo");

    const chunk = {
      id: "session-1",
      data: "Bash(cargo check 2>&1 | tail -40)\n"
    };
    mocks.onData?.(chunk);
    mocks.onData?.(chunk);

    expect(runnerRows()).toHaveLength(1);
  });

  it("stops reporting a task as running the moment its row goes, turn or no turn", async () => {
    await initTaskRunDetection(() => "demo");
    const key = taskKey({
      directory: "demo",
      command: "cargo check"
    });

    mocks.onData?.({
      id: "session-2",
      data: "Bash(cargo check)\n"
    });
    expect(isTaskRunning(key)).toBe(true);

    // The row is what tracks the process — the liveness poll drops it when the
    // process exits, and `stopRunner` drops it on demand. Either way the panel
    // has to follow it, and NOT hold the badge until the agent's turn ends:
    // `sessionStatus` is mocked as `working` for the whole file, so a status-based
    // flag would still read as running here.
    for (const row of runnerRows()) {
      await stopRunner(row.id);
    }

    expect(isTaskRunning(key)).toBe(false);
  });
});
