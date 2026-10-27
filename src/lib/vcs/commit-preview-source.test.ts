import { Command } from "@/lib/ipc/messaging";
import { commitPreviewSource } from "@/lib/vcs/commit-preview-source";
import {
  beforeEach,
  describe,
  expect,
  it,
  vi
} from "vitest";

const mocks = vi.hoisted(() => ({
  sendMessage: vi.fn<(command: string, data?: unknown) => Promise<unknown>>()
}));

vi.mock("@/lib/ipc/messaging", async importOriginal => ({
  ...await importOriginal<typeof import("@/lib/ipc/messaging")>(),
  sendMessage: mocks.sendMessage
}));

const COMMIT = {
  cwd: "C:/repo",
  sha: "abc1234"
};

describe("commit preview source", () => {
  beforeEach(() => {
    mocks.sendMessage.mockReset();
  });

  it("asks for each preview kind at the commit", async () => {
    mocks.sendMessage.mockResolvedValue(null);
    const source = commitPreviewSource(COMMIT);

    await source.image("art/logo.png");
    await source.video("clips/demo.mp4");
    await source.text("README.md");

    function file(path: string) {
      return {
        ...COMMIT,
        path
      };
    }

    expect(mocks.sendMessage).toHaveBeenCalledWith(Command.vcsCommitImage, file("art/logo.png"));
    expect(mocks.sendMessage).toHaveBeenCalledWith(Command.vcsCommitVideo, file("clips/demo.mp4"));
    expect(mocks.sendMessage).toHaveBeenCalledWith(Command.vcsCommitText, file("README.md"));
  });

  it("parses the commit's diff into lines", async () => {
    mocks.sendMessage.mockResolvedValue("@@ -1 +1 @@\n-old\n+new\n");

    const lines = await commitPreviewSource(COMMIT).diff("a.ts");

    expect(mocks.sendMessage).toHaveBeenCalledWith(Command.vcsCommitDiff, {
      ...COMMIT,
      path: "a.ts"
    });
    expect(lines.length).toBeGreaterThan(0);
  });
});
