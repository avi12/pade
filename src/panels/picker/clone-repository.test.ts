import { cloneRepository } from "@/panels/picker/clone-repository";
import {
  beforeEach,
  describe,
  expect,
  it,
  vi
} from "vitest";

const ROOT = "C:\\repositories";
const SSH_URL = "git@github.com:org/poll.git";

const mocks = vi.hoisted(() => ({
  clone: vi.fn(async (data: {
    root: string;
    name: string;
  }) => `${data.root}\\${data.name}`)
}));

vi.mock("@/lib/ipc/messaging", async importOriginal => {
  const messaging = await importOriginal<typeof import("@/lib/ipc/messaging")>();
  return {
    ...messaging,
    sendMessage: vi.fn(async (command: string, data: {
      root: string;
      name: string;
    }) => {
      if (command === messaging.Command.vcsClone) {
        return await mocks.clone(data);
      }

      throw new Error(`unexpected command ${command}`);
    })
  };
});

function request(overrides: Partial<Parameters<typeof cloneRepository>[0]> = {}) {
  return {
    rawUrl: SSH_URL,
    rawName: "poll",
    root: ROOT,
    needsCredentials: false,
    rawUsername: "avi",
    rawPassword: "token",
    onopen: vi.fn(),
    ...overrides
  };
}

beforeEach(() => {
  mocks.clone.mockClear();
});

describe("cloneRepository", () => {
  it("clones without credentials when the SSH key covers it, then opens", async () => {
    const clone = request();
    await expect(cloneRepository(clone)).resolves.toBe("");
    expect(mocks.clone).toHaveBeenCalledWith({
      url: SSH_URL,
      root: ROOT,
      name: "poll",
      username: undefined,
      password: undefined
    });
    expect(clone.onopen).toHaveBeenCalledWith({ path: `${ROOT}\\poll` });
  });

  it("sends the HTTPS credentials when no SSH key is on disk", async () => {
    await cloneRepository(request({ needsCredentials: true }));
    expect(mocks.clone).toHaveBeenCalledWith(
      expect.objectContaining({
        username: "avi",
        password: "token"
      })
    );
  });

  it("does nothing until the fields validate", async () => {
    await expect(cloneRepository(request({ rawName: " " }))).resolves.toBe("");
    expect(mocks.clone).not.toHaveBeenCalled();
  });

  it("surfaces the backend's reason, or a generic one", async () => {
    mocks.clone.mockRejectedValueOnce("Repository not found");
    await expect(cloneRepository(request())).resolves.toBe("Repository not found");
    mocks.clone.mockRejectedValueOnce(new Error("boom"));
    await expect(cloneRepository(request())).resolves.toBe("Clone failed");
  });
});
