import { settings } from "@/lib/settings/settings.svelte";
import type { Ide } from "@/lib/types";
import { createEditorCatalog } from "@/panels/picker/editorCatalog.svelte";
import {
  beforeEach,
  describe,
  expect,
  it,
  vi
} from "vitest";

const SUBLIME_PATH = "C:\\Tools\\sublime_text.exe";

function ide(id: string) {
  return {
    id,
    label: id,
    command: id,
    terminal: false,
    chosen: false
  } satisfies Ide;
}

const detected: { ides: Ide[] } = { ides: [ide("code")] };

const mocks = vi.hoisted(() => ({
  sendSettingsMessage: vi.fn(async () => undefined)
}));

vi.mock("@/lib/ipc/messaging", async importOriginal => {
  const messaging = await importOriginal<typeof import("@/lib/ipc/messaging")>();
  return {
    ...messaging,
    sendMessage: vi.fn(async (command: string) => {
      if (command === messaging.Command.ideDetect) {
        return detected.ides;
      }

      if (command === messaging.Command.ideKinds) {
        return [{
          kind: "node",
          label: "Node",
          signals: ["package.json"]
        }];
      }

      if (command === messaging.Command.ideKindOptions) {
        return { node: ["code"] };
      }

      throw new Error(`unexpected command ${command}`);
    })
  };
});

vi.mock("@/lib/settings/settings-requests", () => ({
  sendSettingsMessage: mocks.sendSettingsMessage
}));

beforeEach(() => {
  detected.ides = [ide("code")];
  settings.prefs.addedEditors = [];
  mocks.sendSettingsMessage.mockReset();
});

describe("editor catalog", () => {
  it("detects editors, kinds and per-kind options together", async () => {
    const catalog = createEditorCatalog();
    await catalog.detect();
    expect(catalog.ides.map(editor => editor.id)).toEqual(["code"]);
    expect(catalog.kinds.map(kind => kind.kind)).toEqual(["node"]);
    expect(catalog.kindOptions).toEqual({ node: ["code"] });
  });

  it("adds an editor and reports its persisted label", async () => {
    mocks.sendSettingsMessage.mockImplementation(async () => {
      settings.prefs.addedEditors = [{
        id: "sublime",
        label: "Sublime Text",
        path: SUBLIME_PATH
      }];
      detected.ides = [ide("code"), ide("sublime")];
    });
    const catalog = createEditorCatalog();
    await expect(catalog.add(` ${SUBLIME_PATH} `)).resolves.toEqual({ label: "Sublime Text" });
    expect(catalog.ides.map(editor => editor.id)).toEqual(["code", "sublime"]);
  });

  it("hands back the backend's rejection", async () => {
    mocks.sendSettingsMessage.mockRejectedValue("Not an executable");
    const catalog = createEditorCatalog();
    await expect(catalog.add(SUBLIME_PATH)).resolves.toEqual({ error: "Not an executable" });
  });

  it("counts only auto-detected editors on a rescan", async () => {
    settings.prefs.addedEditors = [{
      id: "sublime",
      label: "Sublime Text",
      path: SUBLIME_PATH
    }];
    detected.ides = [ide("code"), ide("sublime"), ide("zed")];
    const catalog = createEditorCatalog();
    await expect(catalog.rescan()).resolves.toBe(2);
  });
});
