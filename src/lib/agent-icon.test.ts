import { agentIconName } from "@/lib/agent-icon";
import { describe, expect, it } from "vitest";

describe("agentIconName", () => {
  it("maps known agent ids to their brand glyphs", () => {
    expect(agentIconName("claude")).toBe("claude");
    expect(agentIconName("codex")).toBe("codex");
    expect(agentIconName("copilot")).toBe("copilot");
    expect(agentIconName("grok")).toBe("grok");
    expect(agentIconName("antigravity")).toBe("antigravity");
    expect(agentIconName("cursor")).toBe("cursor");
  });

  it("falls back to the terminal glyph for an agent with no brand mark (aider) or an unknown id", () => {
    expect(agentIconName("aider")).toBe("terminal");
    expect(agentIconName("editor-nvim")).toBe("terminal");
    expect(agentIconName("something-new")).toBe("terminal");
  });
});
