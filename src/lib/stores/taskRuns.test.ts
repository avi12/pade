import { runnerRows } from "@/lib/stores/runners.svelte";
import { initTaskRunDetection } from "@/lib/stores/taskRuns.svelte";
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
    sessionTaskRunning: async () => true
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
});
