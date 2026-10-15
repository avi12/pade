import { activeMemberIn, loadWorkspaceMembers, selectMember } from "@/lib/stores/workspace/workspaceMembers.svelte";
import {
  beforeEach,
  describe,
  expect,
  it,
  vi
} from "vitest";

const ROOT = "C:\\repositories\\avi\\pade";
const OTHER = "C:\\repositories\\avi\\youtube-time-manager";

vi.mock("@/lib/ipc/messaging", async importOriginal => {
  const messaging = await importOriginal<typeof import("@/lib/ipc/messaging")>();
  return {
    ...messaging,
    sendMessage: vi.fn(async (command: string) => {
      if (command === messaging.Command.membersList) {
        return [
          {
            path: "",
            name: "pade",
            ecosystem: "javascript",
            repository: true
          }
        ];
      }

      if (command === messaging.Command.vcsBranchOf) {
        return {};
      }

      throw new Error(`unexpected command ${command}`);
    })
  };
});

describe("activeMemberIn", () => {
  beforeEach(async () => {
    await loadWorkspaceMembers(ROOT);
  });

  it("is the project itself until a member is picked", () => {
    expect(activeMemberIn(ROOT)).toBe(ROOT);
  });

  it("is the picked member once one is", () => {
    const companion = `${ROOT}\\companion`;
    selectMember(companion);
    expect(activeMemberIn(ROOT)).toBe(companion);
  });

  it("never answers for a project it does not hold", () => {
    // Opening a project launches its first agent before the census comes back.
    // Answering with the workspace just left would start the agent in the wrong
    // repository; answering with "" would start it in the user's home.
    selectMember(`${ROOT}\\companion`);
    expect(activeMemberIn(OTHER)).toBe(OTHER);
  });

  it("is the project itself while its own census is still in flight", async () => {
    const pending = loadWorkspaceMembers(OTHER);
    expect(activeMemberIn(OTHER)).toBe(OTHER);
    await pending;
    expect(activeMemberIn(OTHER)).toBe(OTHER);
  });
});
