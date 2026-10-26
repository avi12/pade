import { createTaskCatalog } from "@/lib/stores/tasks/taskCatalog.svelte";
import { knownTaskCommands, taskKey } from "@/lib/stores/tasks/taskRuns.svelte";
import { ManifestMatch, type TaskGroup, TaskManifestDescriptor } from "@/lib/types";
import { describe, expect, it, vi } from "vitest";

const mocks = vi.hoisted(() => ({
  sendMessage: vi.fn(async () => undefined),
  onMessage: vi.fn(async () => () => undefined)
}));

vi.mock("@/lib/ipc/messaging", async importOriginal => ({
  ...await importOriginal<typeof import("@/lib/ipc/messaging")>(),
  sendMessage: mocks.sendMessage,
  onMessage: mocks.onMessage
}));

function group({ project, command }: {
  project: string;
  command: string;
}) {
  return {
    manifest: "package.json",
    dir: project,
    kind: "npm",
    tasks: [{
      name: command,
      command
    }]
  } satisfies TaskGroup;
}

describe("task catalog", () => {
  it("rejects a stale project response that finishes last", async () => {
    let project = "alpha";
    let listCalls = 0;
    const alpha = Promise.withResolvers<TaskGroup[]>();
    const beta = Promise.withResolvers<TaskGroup[]>();
    const catalog = createTaskCatalog({
      project: () => project,
      descriptors: async () => [],
      async list(workspace) {
        listCalls += 1;

        if (listCalls === 1) {
          return [];
        }

        return workspace === "alpha" ? alpha.promise : beta.promise;
      },
      onChange: async () => () => undefined
    });
    await catalog.initialize();

    const staleRefresh = catalog.refresh("alpha");
    project = "beta";
    const currentRefresh = catalog.refresh("beta");
    beta.resolve([group({
      project: "beta",
      command: "pnpm test"
    })]);
    await currentRefresh;
    alpha.resolve([group({
      project: "alpha",
      command: "pnpm build"
    })]);
    await staleRefresh;

    expect(catalog.snapshot.project).toBe("beta");
    expect(catalog.snapshot.groups[0]?.tasks[0]?.command).toBe("pnpm test");
  });

  it("feeds PTY detection from the same accepted group snapshot", async () => {
    const groups = [group({
      project: "demo",
      command: "pnpm check"
    })];
    const catalog = createTaskCatalog({
      project: () => "demo",
      descriptors: async () => [],
      list: async () => groups,
      onChange: async () => () => undefined
    });
    await catalog.initialize();

    expect(catalog.snapshot.groups).toBe(groups);
    expect(knownTaskCommands(catalog.snapshot)).toEqual([{
      key: taskKey({
        directory: "demo",
        command: "pnpm check"
      }),
      command: "pnpm check",
      label: "pnpm check",
      kind: "npm",
      dir: "demo"
    }]);
  });

  it("re-scans when a watched manifest changes, by name or by extension", async () => {
    let listCalls = 0;
    let notify: ((change: {
      root: string;
      path: string;
    }) => void) | undefined;
    const catalog = createTaskCatalog({
      project: () => "demo",
      descriptors: async () => [
        {
          match: ManifestMatch.enum.name,
          value: "package.json"
        },
        {
          match: ManifestMatch.enum.extension,
          value: ".csproj"
        }
      ],
      async list() {
        listCalls += 1;
        return [];
      },
      async onChange(callback) {
        notify = callback;
        return () => undefined;
      }
    });
    await catalog.initialize();
    expect(listCalls).toBe(1);

    notify?.({
      root: "demo",
      path: "C:/demo/package.json"
    });
    notify?.({
      root: "demo",
      path: "C:/demo/src/Renderer.CSPROJ"
    });
    await Promise.resolve();

    expect(listCalls).toBe(3);

    notify?.({
      root: "demo",
      path: "C:/demo/src/Renderer.csproj.user"
    });
    notify?.({
      root: "demo",
      path: "C:/demo/README.md"
    });
    await Promise.resolve();

    expect(listCalls).toBe(3);

    // A project behind the front one has its own manifests; they list nothing here.
    notify?.({
      root: "other",
      path: "C:/other/package.json"
    });
    await Promise.resolve();

    expect(listCalls).toBe(3);
  });

  it("validates descriptor fields at the frontend boundary", () => {
    expect(
      TaskManifestDescriptor.parse({
        match: ManifestMatch.enum.name,
        value: "Makefile"
      })
    ).toEqual({
      match: ManifestMatch.enum.name,
      value: "Makefile"
    });
    expect(
      TaskManifestDescriptor.parse({
        match: ManifestMatch.enum.extension,
        value: ".csproj"
      })
    ).toEqual({
      match: ManifestMatch.enum.extension,
      value: ".csproj"
    });
    expect(() => TaskManifestDescriptor.parse({
      match: ManifestMatch.enum.name,
      value: ""
    })).toThrow();
    expect(() => TaskManifestDescriptor.parse({
      match: "glob",
      value: "*.csproj"
    })).toThrow();
  });

  it("retries a failed subscription without duplicating the successful one", async () => {
    let attempts = 0;
    let activeListeners = 0;
    const catalog = createTaskCatalog({
      project: () => "demo",
      descriptors: async () => [{
        match: ManifestMatch.enum.name,
        value: "Makefile"
      }],
      list: async () => [],
      async onChange() {
        attempts += 1;

        if (attempts === 1) {
          throw new Error("listener unavailable");
        }

        activeListeners += 1;
        return () => {
          activeListeners -= 1;
        };
      }
    });

    await catalog.initialize();
    expect(catalog.snapshot.error).toContain("listener unavailable");
    await catalog.initialize();
    await catalog.initialize();

    expect(attempts).toBe(2);
    expect(activeListeners).toBe(1);
  });
});
