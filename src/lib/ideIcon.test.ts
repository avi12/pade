import { ideIcon } from "@/lib/ideIcon";
import { describe, expect, it } from "vitest";

describe("ideIcon", () => {
  it("maps each detected editor id to its brand mark", () => {
    expect(ideIcon("vscode")).toBe("vscode");
    expect(ideIcon("cursor")).toBe("cursor");
    expect(ideIcon("zed")).toBe("zed");
    expect(ideIcon("sublime")).toBe("sublime");
    expect(ideIcon("visualstudio")).toBe("visualstudio");
    expect(ideIcon("androidstudio")).toBe("androidstudio");
  });

  it("shares the JetBrains mark across the JetBrains IDEs", () => {
    expect(ideIcon("webstorm")).toBe("jetbrains");
    expect(ideIcon("idea")).toBe("jetbrains");
    expect(ideIcon("pycharm")).toBe("jetbrains");
    expect(ideIcon("rustrover")).toBe("jetbrains");
    expect(ideIcon("rider")).toBe("jetbrains");
    expect(ideIcon("clion")).toBe("jetbrains");
    expect(ideIcon("phpstorm")).toBe("jetbrains");
    expect(ideIcon("rubymine")).toBe("jetbrains");
  });

  it("resolves user-added editors by their `added-<basename>` id", () => {
    expect(ideIcon("added-webstorm64")).toBe("jetbrains");
    expect(ideIcon("added-cursor")).toBe("cursor");
    expect(ideIcon("added-sublime_text")).toBe("sublime");
  });

  it("sends console editors to the terminal mark", () => {
    expect(ideIcon("added-nvim")).toBe("terminal");
    expect(ideIcon("added-vim")).toBe("terminal");
    expect(ideIcon("added-hx")).toBe("terminal");
  });

  it("falls back to the generic code glyph for an unknown editor", () => {
    expect(ideIcon("added-notepad++")).toBe("code");
    expect(ideIcon("something-else")).toBe("code");
  });
});
