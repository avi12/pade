import { projectKind, refreshProjectKind, requestProjectKind } from "@/lib/stores/projectKinds.svelte";
import {
  beforeEach,
  describe,
  expect,
  it,
  vi
} from "vitest";

const mocks = vi.hoisted(() => ({
  projectKinds: vi.fn<(paths: string[]) => Promise<Record<string, string>>>()
}));

function deferred<T>() {
  let resolve!: (value: T) => void;
  const promise = new Promise<T>(promiseResolve => {
    resolve = promiseResolve;
  });
  return {
    promise,
    resolve
  };
}

vi.mock("@/lib/bridge", () => ({
  ide: {
    projectKinds: mocks.projectKinds
  }
}));
describe("project kind batches", () => {
  beforeEach(() => {
    mocks.projectKinds.mockReset();
  });

  it("coalesces visible paths into one backend request", async () => {
    const first = "C:\\projects\\lazy-web";
    const second = "C:\\projects\\lazy-rust";
    mocks.projectKinds.mockResolvedValue({
      [first]: "typescript",
      [second]: "rust"
    });

    requestProjectKind(first);
    requestProjectKind(second);

    await vi.waitFor(() => expect(projectKind(second)).toBe("rust"));
    expect(mocks.projectKinds).toHaveBeenCalledOnce();
    expect(mocks.projectKinds).toHaveBeenCalledWith([first, second]);
    expect(projectKind(first)).toBe("typescript");
  });

  it("deduplicates alternate spellings and reuses the cache", async () => {
    const path = "C:\\projects\\same-project";
    mocks.projectKinds.mockResolvedValue({ [path]: "web" });

    requestProjectKind(path);
    requestProjectKind("c:/projects/same-project/");

    await vi.waitFor(() => expect(projectKind(path)).toBe("web"));
    expect(mocks.projectKinds).toHaveBeenCalledOnce();
    requestProjectKind(path);
    await Promise.resolve();
    expect(mocks.projectKinds).toHaveBeenCalledOnce();
  });

  it("publishes an immediate evidence probe through the same cache", async () => {
    const path = "C:\\projects\\new-rust-project";
    mocks.projectKinds.mockResolvedValue({ [path]: "rust" });

    await expect(refreshProjectKind(path)).resolves.toBe("rust");
    expect(projectKind(path)).toBe("rust");

    requestProjectKind(path);
    await Promise.resolve();
    expect(mocks.projectKinds).toHaveBeenCalledOnce();
  });

  it("does not let an older lazy batch overwrite a forced refresh", async () => {
    const path = "C:\\projects\\refresh-race";
    const lazyResponse = deferred<Record<string, string>>();
    const refreshResponse = deferred<Record<string, string>>();
    mocks.projectKinds
      .mockReturnValueOnce(lazyResponse.promise)
      .mockReturnValueOnce(refreshResponse.promise);

    requestProjectKind(path);
    await vi.waitFor(() => expect(mocks.projectKinds).toHaveBeenCalledOnce());

    const refresh = refreshProjectKind(path);
    refreshResponse.resolve({ [path]: "typescript" });
    await expect(refresh).resolves.toBe("typescript");

    lazyResponse.resolve({ [path]: "web" });
    await vi.waitFor(() => expect(projectKind(path)).toBe("typescript"));
    expect(projectKind(path)).toBe("typescript");
  });

  it("does not queue a lazy batch over an active forced refresh", async () => {
    const path = "C:\\projects\\refresh-first";
    const refreshResponse = deferred<Record<string, string>>();
    mocks.projectKinds.mockReturnValueOnce(refreshResponse.promise);

    const refresh = refreshProjectKind(path);
    requestProjectKind(path);
    await Promise.resolve();
    expect(mocks.projectKinds).toHaveBeenCalledOnce();

    refreshResponse.resolve({ [path]: "typescript" });
    await expect(refresh).resolves.toBe("typescript");
    expect(projectKind(path)).toBe("typescript");
  });

  it("publishes only the newest forced refresh for a path", async () => {
    const path = "C:\\projects\\two-refreshes";
    const olderResponse = deferred<Record<string, string>>();
    const newerResponse = deferred<Record<string, string>>();
    mocks.projectKinds
      .mockReturnValueOnce(olderResponse.promise)
      .mockReturnValueOnce(newerResponse.promise);

    const olderRefresh = refreshProjectKind(path);
    const newerRefresh = refreshProjectKind(path);
    newerResponse.resolve({ [path]: "rust" });
    await expect(newerRefresh).resolves.toBe("rust");

    olderResponse.resolve({ [path]: "web" });
    await expect(olderRefresh).resolves.toBe("rust");
    expect(projectKind(path)).toBe("rust");
  });
});
