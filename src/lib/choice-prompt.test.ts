import { detectChoicePrompt } from "@/lib/choice-prompt";
import { describe, expect, it } from "vitest";

const CURSOR = "❯";
// ESC (0x1b) built from its code point so no raw control byte sits in the source.
const ESC = String.fromCharCode(0x1b);

describe("detectChoicePrompt", () => {
  it("flags a numbered select menu with the ❯ cursor", () => {
    const menu = `Do you want to proceed?\n${CURSOR} 1. Yes\n  2. No`;
    expect(detectChoicePrompt(menu)).toBe(true);
  });

  it("flags the menu through the ANSI colour codes a TUI paints it with", () => {
    const painted =
      `${ESC}[2m${CURSOR}${ESC}[0m ${ESC}[1m1.${ESC}[0m Yes  ${ESC}[2m2.${ESC}[0m No`;
    expect(detectChoicePrompt(painted)).toBe(true);
  });

  it("flags a three-option permission prompt", () => {
    const prompt = `${CURSOR} 1. Yes  2. Yes, always  3. No, and tell Claude why`;
    expect(detectChoicePrompt(prompt)).toBe(true);
  });

  it("does NOT flag an ordinary numbered list in prose (no cursor)", () => {
    expect(detectChoicePrompt("Steps:\n1. Install\n2. Build\n3. Ship")).toBe(false);
  });

  it("does NOT flag a lone cursor on a single option", () => {
    expect(detectChoicePrompt(`${CURSOR} 1. Only`)).toBe(false);
  });

  it("does NOT flag output with no selection cursor at all", () => {
    expect(detectChoicePrompt("some agent output with 1. and 2. items")).toBe(false);
  });

  it("does NOT flag the cursor next to non-option text", () => {
    expect(detectChoicePrompt(`${CURSOR} run the build, then 3 files changed`)).toBe(false);
  });
});
