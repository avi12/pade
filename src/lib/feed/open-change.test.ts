import { openAction, openChangedFile } from "@/lib/feed/open-change";
import { Command } from "@/lib/ipc/messaging";
import type { Ide } from "@/lib/types";
import {
  beforeEach,
  describe,
  expect,
  it,
  vi
} from "vitest";

const mocks = vi.hoisted(() => ({
  sendMessage: vi.fn(async () => undefined)
}));

vi.mock("@/lib/ipc/messaging", async importOriginal => ({
  ...await importOriginal<typeof import("@/lib/ipc/messaging")>(),
  sendMessage: mocks.sendMessage
}));

const EDITOR = {
  id: "vscode",
  label: "VS Code",
  command: "code",
  terminal: false,
  chosen: false
} satisfies Ide;

describe("openAction", () => {
  it("shows a binary file in its folder", () => {
    expect(
      openAction({
        path: "dist/app.exe",
        editor: EDITOR
      })
    ).toBe("Show in folder");
  });

  it("names the editor for a text file", () => {
    expect(
      openAction({
        path: "src/main.ts",
        editor: EDITOR
      })
    ).toBe("Open in VS Code");
  });

  it("is empty with no editor to open in", () => {
    expect(
      openAction({
        path: "src/main.ts",
        editor: undefined
      })
    ).toBe("");
  });
});

describe("openChangedFile", () => {
  beforeEach(() => {
    mocks.sendMessage.mockClear();
  });

  it("reveals a binary file in the explorer", async () => {
    await openChangedFile({
      path: "dist/app.zip",
      project: "C:/project",
      editor: EDITOR
    });

    expect(mocks.sendMessage).toHaveBeenCalledWith(Command.revealInExplorer, { path: "dist/app.zip" });
  });

  it("opens a text file in the editor at the line", async () => {
    await openChangedFile({
      path: "src/main.ts",
      line: 12,
      project: "C:/project",
      editor: EDITOR
    });

    expect(mocks.sendMessage).toHaveBeenCalledWith(Command.ideOpenFile, {
      command: "code",
      project: "C:/project",
      file: "src/main.ts",
      line: 12
    });
  });

  it("does nothing for a text file with no editor", async () => {
    await openChangedFile({
      path: "src/main.ts",
      project: "C:/project",
      editor: undefined
    });

    expect(mocks.sendMessage).not.toHaveBeenCalled();
  });
});
