import { describe, expect, it } from "vitest";

import {
  handoffDocName,
  handoffPrompt,
  HandoffReason,
  handoffRequestBody,
  handoffSlug,
  successorPrompt
} from "@/lib/stores/agent/handoff/handoff-prompts";
import { BRACKETED_PASTE_END, PROMPT_SUBMIT } from "@/lib/terminal/input/terminal-input";

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
    const first = handoffDocName({
      source,
      sessionId: "1a2b3c4d-1111-2222-3333-444455556666"
    });
    const second = handoffDocName({
      source,
      sessionId: "9f8e7d6c-1111-2222-3333-444455556666"
    });
    expect(first).toBe("continue-pade-1a2b3c4d.md");
    expect(second).toBe("continue-pade-9f8e7d6c.md");
    expect(first).not.toBe(second);
  });

  it("falls back to a generic token for a non-UUID session id", () => {
    expect(
      handoffDocName({
        source: "pade",
        sessionId: ""
      })
    ).toBe("continue-pade-session.md");
  });
});

describe("handoff prompts", () => {
  const documentName = "continue-pade-1a2b3c4d.md";

  it("explains that an MCP handoff will reload project configuration", () => {
    const prompt = handoffPrompt({
      doc: documentName,
      reason: HandoffReason.ConfigurationChange
    });
    expect(prompt).toContain("MCP server configuration changed");
    expect(prompt).toContain(`handoff to ${documentName}`);
  });

  it("keeps the request body free of paste framing and the submit keystroke", () => {
    // The store pastes this body and submits it with a SEPARATE, retried Enter;
    // a body carrying its own CR or paste marker would defeat that delivery.
    const body = handoffRequestBody({
      doc: documentName,
      reason: HandoffReason.ContextLimit
    });
    expect(body).not.toContain(PROMPT_SUBMIT);
    expect(body).not.toContain(BRACKETED_PASTE_END);
    expect(body).toContain(`handoff to ${documentName}`);
  });

  it("explains a save handoff is for restarting in the saved location", () => {
    const body = handoffRequestBody({
      doc: documentName,
      reason: HandoffReason.Save
    });
    expect(body).toContain("saved as a permanent project");
    expect(body).toContain(`handoff to ${documentName}`);
  });

  it("seeds the successor to continue from the written document", () => {
    expect(successorPrompt(documentName)).toBe(
      `Read ${documentName} to continue the work where the previous session left off.`
    );
  });
});
