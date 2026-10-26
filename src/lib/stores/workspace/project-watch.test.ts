import { Command, sendMessage } from "@/lib/ipc/messaging";
import { heldProjects, rewatchHeldProjects, watchHeldProjects } from "@/lib/stores/workspace/project-watch";
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
});
