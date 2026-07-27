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

  it("matches when the agent wraps the command with cd, args and redirects", () => {
    // The way an agent actually runs a task: a cd prefix, extra flags, a redirect.
    expect(
      isTaskInvocation({
        line: "Bash(cd \"C:/repo/app\" && pnpm dev)",
        command: "pnpm dev"
      })
    ).toBe(true);
    expect(
      isTaskInvocation({
        line: "Bash(cargo check --all-targets 2>&1 | tail -20)",
        command: "cargo check --all-targets"
      })
    ).toBe(true);
    expect(
      isTaskInvocation({
        line: "PowerShell(pnpm build && node scripts/post.mjs)",
        command: "node scripts/post.mjs"
      })
    ).toBe(true);
  });

  it("matches a script run at a prompt behind a cd", () => {
    expect(
      isTaskInvocation({
        line: "$ cd app && node scripts/dev-server.ts",
        command: "node scripts/dev-server.ts"
      })
    ).toBe(true);
  });

  it("still rejects a longer sibling wrapped in a tool call", () => {
    expect(
      isTaskInvocation({
        line: "Bash(cd app && pnpm build:prod)",
        command: "pnpm build"
      })
    ).toBe(false);
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

  it("does not treat an agent's command mention as an invocation", () => {
    expect(
      isTaskInvocation({
        line: "Verified locally with pnpm lint.",
        command: "pnpm lint"
      })
    ).toBe(false);
    expect(
      isTaskInvocation({
        line: "pnpm lint",
        command: "pnpm lint"
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
