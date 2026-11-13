import { describe, expect, it } from "vitest";

import { HandoffReason } from "@/lib/stores/agent/handoff/handoff-prompts";
import { hasUsageHeadroom, pickHandoffSuccessor, pickSuccessor } from "@/lib/stores/agent/handoff/handoff-successor";
import type { Agent } from "@/lib/types/agent";
import { CreditsState } from "@/lib/types/usage";

describe("hasUsageHeadroom", () => {
  it("keeps an agent whose quota still has room", () => {
    expect(
      hasUsageHeadroom({
        usedPercentage: 93,
        credits: CreditsState.enum.off
      })
    ).toBe(true);
  });

  it("keeps an agent whose quota is nearly - but not fully - spent", () => {
    // What sent a handoff to another agent while Claude could still work: its
    // weekly cap read 96%, which is quota left, not quota gone.
    expect(
      hasUsageHeadroom({
        usedPercentage: 96,
        credits: CreditsState.enum.off
      })
    ).toBe(true);
  });

  it("counts a spent quota as headroom while usage credits carry it", () => {
    expect(
      hasUsageHeadroom({
        usedPercentage: 100,
        credits: CreditsState.enum.available
      })
    ).toBe(true);
  });

  it("has none when the quota is spent and credits cannot carry it", () => {
    expect(
      hasUsageHeadroom({
        usedPercentage: 100,
        credits: CreditsState.enum.unavailable
      })
    ).toBe(false);
    expect(
      hasUsageHeadroom({
        usedPercentage: 100,
        credits: null
      })
    ).toBe(false);
  });

  it("treats an unknown quota as enough", () => {
    expect(
      hasUsageHeadroom({
        usedPercentage: null,
        credits: null
      })
    ).toBe(true);
  });
});

describe("pickSuccessor", () => {
  function agent(id: string) {
    return {
      id,
      label: id,
      command: id,
      reordersBidi: false
    } satisfies Agent;
  }

  const claude = agent("claude");
  const codex = agent("codex");
  const gemini = agent("gemini");
  const available = [claude, codex, gemini];
  function headroomFor(ids: string[]) {
    return (agentId: string) => Promise.resolve(ids.includes(agentId));
  }

  it("keeps the current agent while it still has headroom", async () => {
    const chosen = await pickSuccessor({
      current: claude,
      available,
      hasHeadroom: headroomFor(["claude", "codex"])
    });
    expect(chosen).toBe(claude);
  });

  it("crosses over to the first other agent with headroom", async () => {
    const chosen = await pickSuccessor({
      current: claude,
      available,
      hasHeadroom: headroomFor(["codex", "gemini"])
    });
    expect(chosen).toBe(codex);
  });

  it("skips agents without headroom to find one that has it", async () => {
    const chosen = await pickSuccessor({
      current: claude,
      available,
      hasHeadroom: headroomFor(["gemini"])
    });
    expect(chosen).toBe(gemini);
  });

  it("returns null when no agent has headroom", async () => {
    const chosen = await pickSuccessor({
      current: codex,
      available,
      hasHeadroom: headroomFor([])
    });
    expect(chosen).toBeNull();
  });
});

describe("pickHandoffSuccessor", () => {
  const claude: Agent = {
    id: "claude",
    label: "Claude Code",
    command: "claude",
    reordersBidi: true
  };
  const codex: Agent = {
    id: "codex",
    label: "Codex",
    command: "codex",
    reordersBidi: false
  };

  it("pins a configuration handoff to the governed agent", async () => {
    const successor = await pickHandoffSuccessor({
      reason: HandoffReason.CONFIGURATION_CHANGE,
      current: claude,
      available: [claude, codex],
      hasHeadroom: agentId => Promise.resolve(agentId === codex.id)
    });
    expect(successor).toBe(claude);
  });
});
