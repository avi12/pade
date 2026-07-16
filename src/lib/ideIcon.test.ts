import { ideBrand, ideIcon, IdeId } from "@/lib/ideIcon";
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

  it("gives each JetBrains product its own official mark", () => {
    expect(ideIcon("webstorm")).toBe("webstorm");
    expect(ideIcon("idea")).toBe("idea");
    expect(ideIcon("pycharm")).toBe("pycharm");
    expect(ideIcon("rustrover")).toBe("rustrover");
    expect(ideIcon("rider")).toBe("rider");
    expect(ideIcon("clion")).toBe("clion");
    expect(ideIcon("phpstorm")).toBe("phpstorm");
    expect(ideIcon("rubymine")).toBe("rubymine");
  });

  it("resolves user-added editors by their `added-<basename>` id", () => {
    expect(ideIcon("added-webstorm64")).toBe("webstorm");
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

describe("ideBrand", () => {
  it("keys each JetBrains product to its own tint despite the shared mark", () => {
    expect(ideBrand("webstorm")).toBe(IdeId.WebStorm);
    expect(ideBrand("rider")).toBe(IdeId.Rider);
    expect(ideBrand("added-pycharm64")).toBe(IdeId.PyCharm);
  });

  it("resolves user-added aliases to the canonical editor id", () => {
    expect(ideBrand("added-code - insiders")).toBe(IdeId.VsCode);
    expect(ideBrand("added-sublime_text")).toBe(IdeId.Sublime);
    expect(ideBrand("added-devenv")).toBe(IdeId.VisualStudio);
  });

  it("leaves console and unknown editors untinted", () => {
    expect(ideBrand("added-nvim")).toBeUndefined();
    expect(ideBrand("added-notepad++")).toBeUndefined();
    expect(ideBrand("something-else")).toBeUndefined();
  });
});
