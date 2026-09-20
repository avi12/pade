import { createTaskCatalog } from "@/lib/stores/taskCatalog.svelte";
import { knownTaskCommands, taskKey } from "@/lib/stores/taskRuns.svelte";
import { type TaskGroup, TaskManifestDescriptor } from "@/lib/types";
import { describe, expect, it, vi } from "vitest";

vi.mock("@/lib/bridge", () => ({
  feed: {
    onChange: async () => () => undefined
  },
  pty: {
    onData: async () => () => undefined,
    onExit: async () => () => undefined,
    write: async () => undefined
  },
  runner: {
    list: async () => [],
    onData: async () => () => undefined,
    onExit: async () => () => undefined,
    start: async () => undefined,
    stop: async () => undefined
  },
  tasks: {
    descriptors: async () => [],
    list: async () => []
  }
}));

function deferred<T>(): {
  promise: Promise<T>;
  resolve: (value: T) => void;
} {
  let resolvePromise: ((value: T) => void) | undefined;
  const promise = new Promise<T>(resolve => {
    resolvePromise = resolve;
  });
  return {
    promise,
    resolve: value => resolvePromise?.(value)
  };
}

function group({ project, command }: {
  project: string;
  command: string;
}): TaskGroup {
  return {
    manifest: "package.json",
    dir: project,
    kind: "npm",
    tasks: [{
      name: command,
      command
    }]
  };
}

describe("task catalog", () => {
  it("rejects a stale project response that finishes last", async () => {
    let project = "alpha";
    let listCalls = 0;
    const alpha = deferred<TaskGroup[]>();
    const beta = deferred<TaskGroup[]>();
    const catalog = createTaskCatalog({
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
    await catalog.initialize(() => project);

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
      descriptors: async () => [],
      list: async () => groups,
      onChange: async () => () => undefined
    });
    await catalog.initialize(() => "demo");

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
    let notify: ((path: string) => void) | undefined;
    const catalog = createTaskCatalog({
      descriptors: async () => [
        {
          match: "name",
          value: "package.json"
        },
        {
          match: "extension",
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
    await catalog.initialize(() => "demo");
    expect(listCalls).toBe(1);

    notify?.("C:/demo/package.json");
    notify?.("C:/demo/src/Renderer.CSPROJ");
    await Promise.resolve();

    expect(listCalls).toBe(3);

    notify?.("C:/demo/src/Renderer.csproj.user");
    notify?.("C:/demo/README.md");
    await Promise.resolve();

    expect(listCalls).toBe(3);
  });

  it("validates descriptor fields at the frontend boundary", () => {
    expect(
      TaskManifestDescriptor.parse({
        match: "name",
        value: "Makefile"
      })
    ).toEqual({
      match: "name",
      value: "Makefile"
    });
    expect(
      TaskManifestDescriptor.parse({
        match: "extension",
        value: ".csproj"
      })
    ).toEqual({
      match: "extension",
      value: ".csproj"
    });
    expect(() => TaskManifestDescriptor.parse({
      match: "name",
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
      descriptors: async () => [{
        match: "name",
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

    await catalog.initialize(() => "demo");
    expect(catalog.snapshot.error).toContain("listener unavailable");
    await catalog.initialize(() => "demo");
    await catalog.initialize(() => "demo");

    expect(attempts).toBe(2);
    expect(activeListeners).toBe(1);
  });
});
