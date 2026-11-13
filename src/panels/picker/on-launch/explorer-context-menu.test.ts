import { describe, expect, it, vi } from "vitest";

import { setExplorerContextMenu } from "@/panels/picker/on-launch/explorer-context-menu";

const mocks = vi.hoisted(() => ({
  sendMessage: vi.fn(async (_command: string) => undefined)
}));

vi.mock("@/lib/ipc/messaging", async importOriginal => ({
  ...(await importOriginal<typeof import("@/lib/ipc/messaging")>()),
  sendMessage: mocks.sendMessage
}));

describe("setExplorerContextMenu", () => {
  it("registers or unregisters the menu", async () => {
    const { Command } = await import("@/lib/ipc/messaging");
    await expect(setExplorerContextMenu(true)).resolves.toBe("");
    expect(mocks.sendMessage).toHaveBeenLastCalledWith(Command.CONTEXT_MENU_REGISTER);
    await expect(setExplorerContextMenu(false)).resolves.toBe("");
    expect(mocks.sendMessage).toHaveBeenLastCalledWith(Command.CONTEXT_MENU_UNREGISTER);
  });

  it("reports why registering failed", async () => {
    mocks.sendMessage.mockRejectedValueOnce(new Error("Developer Mode is off"));
    await expect(setExplorerContextMenu(true)).resolves.toBe("Developer Mode is off");
    mocks.sendMessage.mockRejectedValueOnce("denied");
    await expect(setExplorerContextMenu(true)).resolves.toBe("denied");
  });
});
