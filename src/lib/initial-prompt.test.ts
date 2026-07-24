import { isTrustGate, promptEchoed } from "@/lib/initial-prompt";
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

describe("promptEchoed", () => {
  const prompt = "add a dark theme toggle to the settings page";

  it("confirms a composer echoing the prompt verbatim", () => {
    expect(
      promptEchoed({
        output: `> ${prompt}`,
        prompt
      })
    ).toBe(true);
  });

  it("sees the echo through ANSI styling and a wrapped composer line", () => {
    const wrapped = "\x1b[36m> add a dark theme\x1b[0m\n  toggle to the settings\n  page";
    expect(
      promptEchoed({
        output: wrapped,
        prompt
      })
    ).toBe(true);
  });

  it("matches a TUI that truncates a long prompt after the prefix", () => {
    expect(
      promptEchoed({
        output: "> add a dark theme toggle to the sett…",
        prompt
      })
    ).toBe(true);
  });

  it("stays false while only the splash is on screen", () => {
    expect(
      promptEchoed({
        output: "Welcome back Avi!\nTips for getting started\n> ",
        prompt
      })
    ).toBe(false);
  });

  it("never confirms on an empty prompt", () => {
    expect(
      promptEchoed({
        output: "anything",
        prompt: ""
      })
    ).toBe(false);
  });
});
