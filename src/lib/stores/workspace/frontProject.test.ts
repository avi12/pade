import { createAutoNamer } from "@/lib/agent/auto-name";
import { EventType } from "@/lib/ipc/messaging";
import { createTaskCatalog } from "@/lib/stores/tasks/taskCatalog.svelte";
import { frontProject, setFrontProjectSource } from "@/lib/stores/workspace/frontProject.svelte";
import { ChangeKind } from "@/lib/types";
import { describe, expect, it, vi } from "vitest";

const TEMP = "C:/Users/me/.pade/workspaces/temp-1";

const mocks = vi.hoisted(() => ({
  sendMessage: vi.fn(async (_command: string, _data?: unknown): Promise<unknown> => null),
  handlers: new Map<string, (message: { data: unknown }) => void>()
}));

vi.mock("@/lib/ipc/messaging", async importOriginal => ({
  ...await importOriginal<typeof import("@/lib/ipc/messaging")>(),
  sendMessage: mocks.sendMessage,
  onMessage: vi.fn(async (event: string, handler: (message: { data: unknown }) => void) => {
    mocks.handlers.set(event, handler);
    return () => mocks.handlers.delete(event);
  })
}));

vi.mock("@/lib/settings/settings-requests", () => ({
  sendSettingsMessage: vi.fn(async () => null)
}));

describe("frontProject", () => {
  it("reads the source the window names, live", () => {
    let active = "C:/work/app";
    setFrontProjectSource(() => active);
    expect(frontProject()).toBe("C:/work/app");

    active = "C:/work/api";
    expect(frontProject()).toBe("C:/work/api");
  });

  it("is what the task catalog lists by default", async () => {
    setFrontProjectSource(() => "C:/work/app");
    const list = vi.fn(async () => []);
    const catalog = createTaskCatalog({
      project: frontProject,
      descriptors: async () => [],
      list,
      onChange: async () => () => undefined
    });
    await catalog.initialize();

    expect(list).toHaveBeenCalledWith("C:/work/app");
    expect(catalog.snapshot.project).toBe("C:/work/app");
  });

  it("is the workspace the auto-namer counts changes for", async () => {
    setFrontProjectSource(() => TEMP);
    const namer = createAutoNamer({
      isOptedOut: () => false,
      labelOf: () => undefined,
      activeAgentCommand: () => "claude"
    });
    await namer.start();
    for (const name of ["a.ts", "b.ts", "c.ts"]) {
      mocks.handlers.get(EventType.feedChange)?.({
        data: {
          id: name,
          root: TEMP,
          path: `${TEMP}/${name}`,
          kind: ChangeKind.enum.created,
          added: 1,
          removed: 0,
          summary: "",
          ts: 0
        }
      });
    }

    await vi.waitFor(() => expect(mocks.sendMessage).toHaveBeenCalledWith(
      expect.anything(),
      expect.objectContaining({ path: TEMP })
    ));
    namer.dispose();
  });
});
