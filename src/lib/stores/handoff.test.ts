import { handoffDocName, handoffSlug, pickSuccessor } from "@/lib/stores/handoff.svelte";
import type { Agent } from "@/lib/types";
import { describe, expect, it } from "vitest";

describe("handoffSlug", () => {
  it("lowercases and kebabs a workspace label", () => {
    expect(handoffSlug("My Project!")).toBe("my-project");
  });

  it("flattens path separators from a short dir", () => {
    expect(handoffSlug("avi/pade")).toBe("avi-pade");
  });

  it("keeps an already-safe label unchanged", () => {
    expect(handoffSlug("temp-42")).toBe("temp-42");
  });

  it("collapses punctuation runs and strips the edges", () => {
    expect(handoffSlug("--wip: retry loop--")).toBe("wip-retry-loop");
  });

  it("falls back to a generic slug when nothing survives", () => {
    expect(handoffSlug("!!!")).toBe("session");
    expect(handoffSlug("")).toBe("session");
  });
});

describe("handoffDocName", () => {
  it("includes the session token so same-project sessions never collide", () => {
    const source = "pade";
    const first = handoffDocName({ source, sessionId: "1a2b3c4d-1111-2222-3333-444455556666" });
    const second = handoffDocName({ source, sessionId: "9f8e7d6c-1111-2222-3333-444455556666" });
    expect(first).toBe("continue-pade-1a2b3c4d.md");
    expect(second).toBe("continue-pade-9f8e7d6c.md");
    expect(first).not.toBe(second);
  });

  it("falls back to a generic token for a non-UUID session id", () => {
    expect(handoffDocName({ source: "pade", sessionId: "" })).toBe("continue-pade-session.md");
  });
});

describe("pickSuccessor", () => {
  function agent(id: string): Agent {
    return {
      id,
      label: id,
      command: id
    };
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
