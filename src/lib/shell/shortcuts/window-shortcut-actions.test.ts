import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import { Command, sendMessage } from "@/lib/ipc/messaging";
import type * as messagingModule from "@/lib/ipc/messaging";
import { PROJECT_SEARCH_ID, runWindowShortcut } from "@/lib/shell/shortcuts/window-shortcut-actions";
import { WindowShortcut } from "@/lib/shell/shortcuts/window-shortcuts";
import { toastText } from "@/lib/stores/ui/toast.svelte";
import { WindowMode } from "@/lib/types/window";

vi.mock("@/lib/ipc/messaging", async importOriginal => {
  const messaging = await importOriginal<typeof messagingModule>();
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

    await runWindowShortcut(WindowShortcut.RELOAD);

    expect(reload).toHaveBeenCalledOnce();
    expect(sendMessageMock).not.toHaveBeenCalled();
  });

  it("opens a fresh empty window and says so", async () => {
    await runWindowShortcut(WindowShortcut.NEW_WINDOW);

    expect(sendMessageMock).toHaveBeenCalledWith(Command.WINDOW_CREATE, { mode: WindowMode.enum.empty });
    expect(toastText()).toBe("Opened a new window");
  });

  it("focuses and selects the project search", async () => {
    const focus = vi.fn();
    const select = vi.fn();
    const querySelector = vi.fn(() => ({
      focus,
      select
    }));
    vi.stubGlobal("document", { querySelector });

    await runWindowShortcut(WindowShortcut.FIND_PROJECT);

    expect(querySelector).toHaveBeenCalledWith(`input#${PROJECT_SEARCH_ID}`);
    expect(focus).toHaveBeenCalledOnce();
    expect(select).toHaveBeenCalledOnce();
    expect(sendMessageMock).not.toHaveBeenCalled();
  });

  it("cycles to the previous window", async () => {
    await runWindowShortcut(WindowShortcut.PREVIOUS_WINDOW);

    expect(sendMessageMock).toHaveBeenCalledWith(Command.WINDOW_FOCUS_RELATIVE, { direction: "previous" });
  });

  it("cycles to the next window", async () => {
    await runWindowShortcut(WindowShortcut.NEXT_WINDOW);

    expect(sendMessageMock).toHaveBeenCalledWith(Command.WINDOW_FOCUS_RELATIVE, { direction: "next" });
  });
});
