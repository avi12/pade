import { saveTempWorkspace } from "@/lib/save-temp-workspace";
import type { AgentSession } from "@/lib/types";
import {
  beforeEach,
  describe,
  expect,
  it,
  vi
} from "vitest";

const calls: string[] = [];
const migration: {
  path: string;
  install: string | null;
} = {
  path: "C:\\Projects\\poll",
  install: null
};

vi.mock("@/lib/bridge", () => ({
  workspace: {
    saveMigrate: vi.fn(async () => {
      calls.push("copy");
      return migration;
    }),
    delete: vi.fn(async () => {
      calls.push("delete temp");
      throw new Error("folder busy");
    })
  }
}));

vi.mock("@/lib/stores/runners.svelte", () => ({
  startRunner: vi.fn(async ({ command }: { command: string }) => {
    calls.push(`run ${command}`);
  })
}));

vi.mock("@/lib/stores/handoff.svelte", () => ({
  successorPrompt: (doc: string) => `continue from ${doc}`
}));

const TEMP = "C:\\Users\\Avi\\AppData\\Roaming\\pade\\workspaces\\temp-1";
const SESSION: AgentSession = {
  id: "a",
  agent: {
    id: "claude",
    label: "Claude Code",
    command: "claude",
    reordersBidi: true
  },
  conversationId: "conversation-a"
};

function host(continuing: AgentSession | undefined) {
  return {
    project: TEMP,
    sessionIds: ["a", "b"],
    continuing,
    writeHandoffDoc: vi.fn(async () => {
      calls.push("handoff");
      return "HANDOFF.md";
    }),
    reroot: vi.fn((path: string) => calls.push(`reroot ${path}`)),
    launch: vi.fn(({ initialPrompt }: { initialPrompt?: string }) => {
      calls.push(`launch ${initialPrompt ?? ""}`.trim());
    }),
    closeSessions: vi.fn(async (ids: readonly string[]) => {
      calls.push(`close ${ids.join(",")}`);
    })
  };
}

describe("saveTempWorkspace", () => {
  beforeEach(() => {
    calls.length = 0;
    migration.install = null;
  });

  it("hands off, copies, launches the successor, then tears the temp down", async () => {
    await saveTempWorkspace({
      name: "poll",
      host: host(SESSION)
    });
    expect(calls).toEqual([
      "handoff",
      "copy",
      `reroot ${migration.path}`,
      "launch continue from HANDOFF.md",
      "close a,b",
      "delete temp"
    ]);
  });

  it("installs dependencies when the saved project asks for it", async () => {
    migration.install = "pnpm install";
    await saveTempWorkspace({
      name: "poll",
      host: host(SESSION)
    });
    expect(calls.at(-1)).toBe("run pnpm install");
  });

  it("launches nothing without a session to carry on", async () => {
    const saveHost = host(undefined);
    await saveTempWorkspace({
      name: "poll",
      host: saveHost
    });
    expect(saveHost.writeHandoffDoc).not.toHaveBeenCalled();
    expect(saveHost.launch).not.toHaveBeenCalled();
  });
});
