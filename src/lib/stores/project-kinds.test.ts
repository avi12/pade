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
});
