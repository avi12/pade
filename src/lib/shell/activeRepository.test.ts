import { Command, sendMessage } from "@/lib/ipc/messaging";
import { createActiveRepository } from "@/lib/shell/activeRepository.svelte";
import {
  beforeEach,
  describe,
  expect,
  it,
  vi
} from "vitest";

const ROOT = "C:\\repositories\\avi\\pade";
const OTHER = "C:\\repositories\\avi\\youtube-time-manager";
const REMOTE_URL = "https://github.com/avi/pade.git";

vi.mock("@/lib/ipc/messaging", async importOriginal => {
  const messaging = await importOriginal<typeof import("@/lib/ipc/messaging")>();
  return {
    ...messaging,
    sendMessage: vi.fn()
  };
});

const sendMessageMock = vi.mocked(sendMessage);

function answerAsRepository() {
  sendMessageMock.mockImplementation(async (command, ...data) => {
    if (command === Command.vcsBranches) {
      return ["main", "feature"];
    }

    if (command === Command.vcsRemoteBranches) {
      return ["origin-only"];
    }

    if (command === Command.vcsBranchOf) {
      const [{ paths }] = data as [{ paths: string[] }];
      return { [paths[0]]: "main" };
    }

    if (command === Command.vcsRemoteUrl) {
      return REMOTE_URL;
    }

    throw new Error(`unexpected command ${command}`);
  });
}

describe("createActiveRepository", () => {
  beforeEach(() => {
    sendMessageMock.mockReset();
  });

  it("reads the branches, the checked-out branch and the remote", async () => {
    answerAsRepository();
    const repository = createActiveRepository({ project: () => ROOT });

    await repository.load();

    expect(repository.branches).toEqual(["main", "feature"]);
    expect(repository.remoteBranches).toEqual(["origin-only"]);
    expect(repository.currentBranch).toBe("main");
    expect(repository.hasRemote).toBe(true);
  });

  it("reads a folder that is not a repository as no branches and no remote", async () => {
    sendMessageMock.mockRejectedValue(new Error("not a git repository"));
    const repository = createActiveRepository({ project: () => ROOT });

    await repository.load();

    expect(repository.branches).toEqual([]);
    expect(repository.remoteBranches).toEqual([]);
    expect(repository.currentBranch).toBe("");
    expect(repository.hasRemote).toBe(false);
  });

  it("drops a read the project switch outran", async () => {
    answerAsRepository();
    let project = ROOT;
    const repository = createActiveRepository({ project: () => project });

    const loading = repository.load();
    project = OTHER;
    await loading;

    expect(repository.branches).toEqual([]);
    expect(repository.currentBranch).toBe("");
  });

  it("forgets everything once the window holds no project", async () => {
    answerAsRepository();
    let project = ROOT;
    const repository = createActiveRepository({ project: () => project });
    await repository.load();

    project = "";
    await repository.load();

    expect(repository.branches).toEqual([]);
    expect(repository.remoteBranches).toEqual([]);
    expect(repository.currentBranch).toBe("");
    expect(repository.hasRemote).toBe(false);
  });

  it("clears on request", async () => {
    answerAsRepository();
    const repository = createActiveRepository({ project: () => ROOT });
    await repository.load();

    repository.clear();

    expect(repository.branches).toEqual([]);
    expect(repository.hasRemote).toBe(false);
  });
});
