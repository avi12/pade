import { Command, sendMessage } from "@/lib/ipc/messaging";
import { runWindowShortcut } from "@/lib/shell/window-shortcut-actions";
import { WindowShortcut } from "@/lib/shell/window-shortcuts";
import { toastText } from "@/lib/stores/ui/toast.svelte";
import { WindowMode } from "@/lib/types";
import {
  afterEach,
  beforeEach,
  describe,
  expect,
  it,
  vi
} from "vitest";

vi.mock("@/lib/ipc/messaging", async importOriginal => {
  const messaging = await importOriginal<typeof import("@/lib/ipc/messaging")>();
  return {
    ...messaging,
    sendMessage: vi.fn(async () => {})
  };
});

const sendMessageMock = vi.mocked(sendMessage);

describe("runWindowShortcut", () => {
  beforeEach(() => {
    sendMessageMock.mockClear();
  });

  afterEach(() => {
    vi.unstubAllGlobals();
  });

  it("reloads the WebView", async () => {
    const reload = vi.fn();
    vi.stubGlobal("location", { reload });

    await runWindowShortcut(WindowShortcut.Reload);

    expect(reload).toHaveBeenCalledOnce();
    expect(sendMessageMock).not.toHaveBeenCalled();
  });

  it("opens a fresh empty window and says so", async () => {
    await runWindowShortcut(WindowShortcut.NewWindow);

    expect(sendMessageMock).toHaveBeenCalledWith(Command.windowCreate, { mode: WindowMode.enum.empty });
    expect(toastText()).toBe("Opened a new window");
  });

  it("cycles to the previous window", async () => {
    await runWindowShortcut(WindowShortcut.PreviousWindow);

    expect(sendMessageMock).toHaveBeenCalledWith(Command.windowFocusRelative, { direction: "previous" });
  });

  it("cycles to the next window", async () => {
    await runWindowShortcut(WindowShortcut.NextWindow);

    expect(sendMessageMock).toHaveBeenCalledWith(Command.windowFocusRelative, { direction: "next" });
  });
});
