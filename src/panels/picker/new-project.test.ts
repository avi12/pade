import type { PathProbe } from "@/lib/types";
import { childPath } from "@/lib/util/paths";
import { startTempWorkspace, submitNewProject } from "@/panels/picker/new-project";
import {
  beforeEach,
  describe,
  expect,
  it,
  vi
} from "vitest";

const ROOT = "C:\\repositories";
const TEMP = "C:\\temp\\workspace-1";
const disk: { probe: Pick<PathProbe, "isDir" | "isFile"> } = {
  probe: {
    isDir: false,
    isFile: false
  }
};
const created: string[] = [];

vi.mock("@/lib/ipc/messaging", async importOriginal => {
  const messaging = await importOriginal<typeof import("@/lib/ipc/messaging")>();
  return {
    ...messaging,
    sendMessage: vi.fn(async (command: string, data: {
      root: string;
      name: string;
    }) => {
      if (command === messaging.Command.workspaceTemp) {
        return TEMP;
      }

      if (command === messaging.Command.workspaceProbePath) {
        return disk.probe;
      }

      if (command === messaging.Command.workspaceCreate) {
        created.push(data.name);
        return `${data.root}\\${data.name}`;
      }

      throw new Error(`unexpected command ${command}`);
    })
  };
});

function handoff() {
  return {
    agent: "claude",
    onopen: vi.fn(),
    onprobe: vi.fn()
  };
}

beforeEach(() => {
  disk.probe = {
    isDir: false,
    isFile: false
  };
  created.length = 0;
});

describe("submitNewProject", () => {
  it("falls through to a temp workspace when the name is blank", async () => {
    const callbacks = handoff();
    await submitNewProject({
      root: ROOT,
      rawName: "  ",
      rawPrompt: "scaffold an app",
      ...callbacks
    });
    expect(callbacks.onopen).toHaveBeenCalledWith({
      path: TEMP,
      initialPrompt: "scaffold an app",
      agent: "claude",
      created: true
    });
    expect(callbacks.onprobe).not.toHaveBeenCalled();
  });

  it("creates the project after an authoritative probe", async () => {
    const callbacks = handoff();
    await submitNewProject({
      root: ROOT,
      rawName: "poll",
      rawPrompt: "",
      ...callbacks
    });
    expect(callbacks.onprobe).toHaveBeenCalledWith({
      path: childPath({
        parent: ROOT,
        name: "poll"
      }),
      isDir: false,
      isFile: false
    });
    expect(created).toEqual(["poll"]);
    expect(callbacks.onopen).toHaveBeenCalledWith({
      path: `${ROOT}\\poll`,
      initialPrompt: undefined,
      agent: "claude",
      created: true
    });
  });

  it("never creates over an existing folder", async () => {
    disk.probe = {
      isDir: true,
      isFile: false
    };
    const callbacks = handoff();
    await submitNewProject({
      root: ROOT,
      rawName: "poll",
      rawPrompt: "",
      ...callbacks
    });
    expect(callbacks.onprobe).toHaveBeenCalledOnce();
    expect(created).toEqual([]);
    expect(callbacks.onopen).not.toHaveBeenCalled();
  });

  it("needs a root for a named project", async () => {
    const callbacks = handoff();
    await submitNewProject({
      root: "",
      rawName: "poll",
      rawPrompt: "",
      ...callbacks
    });
    expect(callbacks.onprobe).not.toHaveBeenCalled();
    expect(callbacks.onopen).not.toHaveBeenCalled();
  });
});

describe("startTempWorkspace", () => {
  it("opens a temp workspace with no agent pick", async () => {
    const onopen = vi.fn();
    await startTempWorkspace({
      rawPrompt: "",
      agent: null,
      onopen
    });
    expect(onopen).toHaveBeenCalledWith({
      path: TEMP,
      initialPrompt: undefined,
      agent: undefined,
      created: true
    });
  });
});
