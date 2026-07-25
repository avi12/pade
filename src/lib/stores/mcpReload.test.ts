import { parseMcpReloadFailure } from "@/lib/stores/mcpReload.svelte";
import { describe, expect, it } from "vitest";

describe("parseMcpReloadFailure", () => {
  it.each([
    "MCP server reload failed",
    "Error: unable to initialize MCP server github",
    "Invalid MCP configuration"
  ])("recognizes an MCP reload failure: %s", text => {
    expect(parseMcpReloadFailure({ text })).toBe(true);
  });

  it.each([
    "MCP server github connected",
    "API error: request failed",
    "Updated opencode.json"
  ])("rejects unrelated or successful output: %s", text => {
    expect(parseMcpReloadFailure({ text })).toBe(false);
  });
});
