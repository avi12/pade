import { promptEchoed, TrustGateKey, trustGateKey } from "@/lib/initial-prompt";
import { describe, expect, it } from "vitest";

// Claude Code 2.1.286's first-run gate as rendered: unnumbered options, with the
// cursor starting on "No, exit" ABOVE the accepting option.
const GATE_HEADER = [
  " Accessing workspace:",
  "",
  " Quick safety check: Is this a project you created or one you trust?",
  " Claude Code'll be able to read, edit, and execute files here.",
  ""
];
const GATE_FOOTER = ["", " Enter to confirm · Esc to cancel"];

function gate(options: string[]): string[] {
  return [...GATE_HEADER, ...options, ...GATE_FOOTER];
}

describe("trustGateKey", () => {
  it("steps down when the cursor starts on 'No, exit' above the trust option", () => {
    expect(trustGateKey(gate([" ❯ No, exit", "   Yes, I trust this folder"]))).toBe(TrustGateKey.Down);
  });

  it("confirms once the cursor sits on the trust option", () => {
    expect(trustGateKey(gate(["   No, exit", " ❯ Yes, I trust this folder"]))).toBe(TrustGateKey.Confirm);
  });

  it("confirms the older numbered gate whose default is already trust", () => {
    expect(trustGateKey(gate([" ❯ 1. Yes, I trust this folder", "   2. No, exit"]))).toBe(TrustGateKey.Confirm);
  });

  it("steps up when the trust option sits above the cursor", () => {
    expect(trustGateKey(gate(["   1. Yes, I trust this folder", " ❯ 2. No, exit"]))).toBe(TrustGateKey.Up);
  });

  it("ignores a real multiple-choice question the agent asks later", () => {
    // A genuine choice prompt — must NOT auto-answer; the user answers this one.
    expect(trustGateKey(["❯ 1. Overwrite the file", "  2. Keep both", "  3. Cancel"])).toBeNull();
  });

  it("ignores prose that merely mentions trust", () => {
    expect(trustGateKey(["I don't trust this regex, let me rewrite it.", "❯ "])).toBeNull();
  });

  it("ignores the REPL's input cursor far from a quoted gate line", () => {
    const rows = ["Claude asked: Yes, I trust this folder", ...Array.from({ length: 10 }, () => ""), "❯ Try something"];
    expect(trustGateKey(rows)).toBeNull();
  });

  it("is null for an empty screen", () => {
    expect(trustGateKey([])).toBeNull();
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
