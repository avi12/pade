import { isTaskInvocation } from "@/lib/task-detect";
import { describe, expect, it } from "vitest";

// ESC (0x1b) built from its code point so no raw control byte sits in the source.
const ESC = String.fromCharCode(0x1b);

describe("isTaskInvocation", () => {
  it("matches a bare shell invocation", () => {
    expect(
      isTaskInvocation({
        line: "$ pnpm dev",
        command: "pnpm dev"
      })
    ).toBe(true);
  });

  it("matches the agent's Tool(command) rendering", () => {
    expect(
      isTaskInvocation({
        line: "PowerShell(pnpm dev)",
        command: "pnpm dev"
      })
    ).toBe(true);
    expect(
      isTaskInvocation({
        line: "Bash(pnpm build)",
        command: "pnpm build"
      })
    ).toBe(true);
  });

  it("matches through the ANSI codes the transcript is painted with", () => {
    const painted = `${ESC}[1mPowerShell${ESC}[0m(${ESC}[36mpnpm dev${ESC}[0m)`;
    expect(
      isTaskInvocation({
        line: painted,
        command: "pnpm dev"
      })
    ).toBe(true);
  });

  it("does not match a longer sibling command", () => {
    expect(
      isTaskInvocation({
        line: "pnpm build:prod",
        command: "pnpm build"
      })
    ).toBe(false);
  });

  it("does not match a command embedded in a longer word", () => {
    expect(
      isTaskInvocation({
        line: "run xpnpm devy",
        command: "pnpm dev"
      })
    ).toBe(false);
  });

  it("does not match when the command is absent", () => {
    expect(
      isTaskInvocation({
        line: "just some other output",
        command: "pnpm dev"
      })
    ).toBe(false);
  });
});
