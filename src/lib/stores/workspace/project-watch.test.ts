import { Command, sendMessage } from "@/lib/ipc/messaging";
import {
  heldProjects,
  onWatchesArmed,
  rewatchHeldProjects,
  watchHeldProjects
} from "@/lib/stores/workspace/project-watch";
import { describe, expect, it, vi } from "vitest";

const APP = "C:/work/app";
const API = "C:/work/api";

vi.mock("@/lib/ipc/messaging", async importOriginal => ({
  ...await importOriginal<typeof import("@/lib/ipc/messaging")>(),
  sendMessage: vi.fn(async () => null)
}));

describe("heldProjects", () => {
  it("is every session's project once, plus the front one before it has a session", () => {
    expect(
      heldProjects({
        sessionProjects: [APP, API, APP],
        frontProject: "C:/work/new"
      })
    ).toEqual([API, APP, "C:/work/new"]);
  });

  it("holds nothing while no project is in front and no session runs", () => {
    expect(
      heldProjects({
        sessionProjects: [],
        frontProject: ""
      })
    ).toEqual([]);
  });
});

describe("watchHeldProjects", () => {
  it("asks the backend only when the held set changes, and again on demand", async () => {
    await watchHeldProjects([API, APP]);
    await watchHeldProjects([API, APP]);
    await watchHeldProjects([APP]);
    await rewatchHeldProjects();

    expect(vi.mocked(sendMessage).mock.calls).toEqual([
      [Command.watchProjects, { roots: [API, APP] }],
      [Command.watchProjects, { roots: [APP] }],
      [Command.watchProjects, { roots: [APP] }]
    ]);
  });

  it("tells subscribers each time a request settles, failed ones included", async () => {
    const listener = vi.fn(async () => {});
    const unsubscribe = onWatchesArmed(listener);
    vi.mocked(sendMessage).mockRejectedValueOnce(new Error("locked"));

    await expect(watchHeldProjects([API])).rejects.toThrow("locked");
    await rewatchHeldProjects();
    unsubscribe();
    await rewatchHeldProjects();

    expect(listener).toHaveBeenCalledTimes(2);
  });

  it("retries a set whose last request failed", async () => {
    vi.mocked(sendMessage).mockClear();
    vi.mocked(sendMessage).mockRejectedValueOnce(new Error("locked"));

    await expect(watchHeldProjects(["C:/work/retry"])).rejects.toThrow("locked");
    await watchHeldProjects(["C:/work/retry"]);

    expect(vi.mocked(sendMessage)).toHaveBeenCalledTimes(2);
  });
});
