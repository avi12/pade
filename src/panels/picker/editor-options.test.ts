import type { Ide } from "@/lib/types";
import { editorSelectId, editorsForKind } from "@/panels/picker/editor-options";
import { describe, expect, it } from "vitest";

function ide(id: string) {
  return {
    id,
    label: id,
    command: id,
    terminal: false,
    chosen: false
  } satisfies Ide;
}

const IDES = [ide("code"), ide("webstorm"), ide("android-studio")];

describe("editorsForKind", () => {
  it("offers the kind's fitting editors in priority order", () => {
    expect(
      editorsForKind({
        ides: IDES,
        kindOptions: {
          node: ["webstorm", "code"]
        },
        kind: "node"
      }).map(editor => editor.id)
    ).toEqual(["webstorm", "code"]);
  });

  it("skips an editor that's no longer detected", () => {
    expect(
      editorsForKind({
        ides: IDES,
        kindOptions: {
          rust: ["rustrover", "code"]
        },
        kind: "rust"
      }).map(editor => editor.id)
    ).toEqual(["code"]);
  });

  it("offers every editor for an unknown kind", () => {
    expect(
      editorsForKind({
        ides: IDES,
        kindOptions: {},
        kind: "fallback"
      })
    ).toBe(IDES);
  });
});

describe("editorSelectId", () => {
  it("makes a valid id from any kind", () => {
    expect(editorSelectId("c++/cmake")).toBe("ide-c---cmake");
    expect(editorSelectId("fallback")).toBe("ide-fallback");
  });
});
