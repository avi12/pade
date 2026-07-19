import { isTrustGate } from "@/lib/initial-prompt";
import { describe, expect, it } from "vitest";

// Claude Code's first-run gate, roughly as it paints it (cursor on the default).
const TRUST_GATE = [
  "Quick safety check: Is this a project you created or one you trust?",
  "Claude Code'll be able to read, edit, and execute files here.",
  "❯ 1. Yes, I trust this folder",
  "  2. No, exit",
  "Enter to confirm · Esc to cancel"
].join("\n");

describe("isTrustGate", () => {
  it("recognizes the trust-folder gate", () => {
    expect(isTrustGate(TRUST_GATE)).toBe(true);
  });

  it("sees it through ANSI colour codes", () => {
    const coloured = `\x1b[1m❯ 1.\x1b[0m Yes, I \x1b[33mtrust\x1b[0m this folder\n  2. No, exit`;
    expect(isTrustGate(coloured)).toBe(true);
  });

  it("ignores a real multiple-choice question the agent asks later", () => {
    // A genuine choice prompt — must NOT auto-accept; the user answers this one.
    const question = "❯ 1. Overwrite the file\n  2. Keep both\n  3. Cancel";
    expect(isTrustGate(question)).toBe(false);
  });

  it("ignores prose that merely mentions trust", () => {
    expect(isTrustGate("I don't trust this regex, let me rewrite it.")).toBe(false);
  });

  it("ignores the input line's plain prompt caret", () => {
    // The REPL's own "> " is not the U+276F selection cursor of a menu.
    expect(isTrustGate("> do you trust this? type your answer")).toBe(false);
  });

  it("is false for empty output", () => {
    expect(isTrustGate("")).toBe(false);
  });
});
