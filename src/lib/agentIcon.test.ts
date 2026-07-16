import { agentIconName } from "@/lib/agentIcon";
import { describe, expect, it } from "vitest";

describe("agentIconName", () => {
  it("maps known agent ids to their glyphs", () => {
    expect(agentIconName("claude")).toBe("sparkles");
    expect(agentIconName("codex")).toBe("code");
    expect(agentIconName("grok")).toBe("activity");
    expect(agentIconName("aider")).toBe("git");
  });

  it("falls back to the terminal glyph for an unknown id", () => {
    expect(agentIconName("editor-nvim")).toBe("terminal");
    expect(agentIconName("something-new")).toBe("terminal");
  });
});
