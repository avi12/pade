import { isNameTakenIn, prefillableName, probeSaveTargets } from "@/lib/workspace/save-workspace-form";
import { describe, expect, it, vi } from "vitest";

const mocks = vi.hoisted(() => ({ sendMessage: vi.fn() }));

vi.mock("@/lib/ipc/messaging", async importOriginal => ({
  ...await importOriginal<typeof import("@/lib/ipc/messaging")>(),
  sendMessage: mocks.sendMessage
}));

const ROOT = "C:\\repositories";
const OTHER_ROOT = "D:\\work";

describe("prefillableName", () => {
  it("offers a friendly label", () => {
    expect(prefillableName("  poll-board ")).toBe("poll-board");
  });

  it("never offers the raw temp folder name", () => {
    expect(prefillableName("temp-1792084338")).toBeNull();
  });

  it("never offers a label that is not a valid name", () => {
    expect(prefillableName("a/b")).toBeNull();
  });
});

describe("probeSaveTargets", () => {
  it("reports which roots already hold the name", async () => {
    mocks.sendMessage.mockImplementation(async (_command: string, { path }: { path: string }) => ({
      isDir: path.startsWith(ROOT),
      isFile: false
    }));
    expect(
      await probeSaveTargets({
        name: "poll-board",
        roots: [ROOT, OTHER_ROOT]
      })
    ).toEqual([
      {
        name: "poll-board",
        root: ROOT,
        exists: true
      },
      {
        name: "poll-board",
        root: OTHER_ROOT,
        exists: false
      }
    ]);
  });

  it("treats an unreachable probe as free", async () => {
    mocks.sendMessage.mockRejectedValue(new Error("offline"));
    expect(
      await probeSaveTargets({
        name: "poll-board",
        roots: [ROOT]
      })
    ).toEqual([{
      name: "poll-board",
      root: ROOT,
      exists: false
    }]);
  });
});

describe("isNameTakenIn", () => {
  const probes = [{
    name: "poll-board",
    root: ROOT,
    exists: true
  }];

  it("finds the name taken in its root", () => {
    expect(
      isNameTakenIn({
        probes,
        root: ROOT,
        name: "poll-board"
      })
    ).toBe(true);
  });

  it("ignores a probe for another name or root", () => {
    expect(
      isNameTakenIn({
        probes,
        root: ROOT,
        name: "other"
      })
    ).toBe(false);
    expect(
      isNameTakenIn({
        probes,
        root: OTHER_ROOT,
        name: "poll-board"
      })
    ).toBe(false);
  });
});
