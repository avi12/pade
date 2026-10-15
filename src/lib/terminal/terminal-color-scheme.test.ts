import { AgentId } from "@/lib/agent/agent-icon";
import {
  colorSchemeReport,
  createColorSchemeRelay,
  enablesColorSchemeNotifications
} from "@/lib/terminal/terminal-color-scheme";
import { describe, expect, it, vi } from "vitest";

describe("colorSchemeReport", () => {
  it("reports dark and light through the DEC 2031 status channel", () => {
    expect(colorSchemeReport("dark")).toBe("\x1b[?997;1n");
    expect(colorSchemeReport("light")).toBe("\x1b[?997;2n");
  });

  it("recognises a TUI enabling live color-scheme notifications", () => {
    expect(enablesColorSchemeNotifications("before\x1b[?2031hafter")).toBe(true);
    expect(enablesColorSchemeNotifications("\x1b[?2031l")).toBe(false);
  });
});

function relayFor(agentId: string) {
  const report = vi.fn(async () => {});
  const relay = createColorSchemeRelay({
    agentId: () => agentId,
    report
  });
  return {
    relay,
    report
  };
}

describe("createColorSchemeRelay", () => {
  it("reports once when the handshake arrives split across two chunks", () => {
    const { relay, report } = relayFor(AgentId.Codex);
    relay.observe("output\x1b[?20");
    expect(report).not.toHaveBeenCalled();

    relay.observe("31h more");
    relay.observe("\x1b[?2031h");
    expect(report).toHaveBeenCalledTimes(1);
  });

  it("relays a palette change only once the agent subscribed", () => {
    const { relay, report } = relayFor(AgentId.Codex);
    relay.relayChange();
    expect(report).not.toHaveBeenCalled();

    relay.observe("\x1b[?2031h");
    relay.relayChange();
    expect(report).toHaveBeenCalledTimes(2);
  });

  it("reports to a known subscriber without a handshake, exactly once", async () => {
    const { relay, report } = relayFor(AgentId.Claude);
    await relay.reportToSubscribedAgent();
    await relay.reportToSubscribedAgent();
    expect(report).toHaveBeenCalledTimes(1);
  });

  it("leaves an agent that never subscribes alone", async () => {
    const { relay, report } = relayFor(AgentId.Codex);
    await relay.reportToSubscribedAgent();
    expect(report).not.toHaveBeenCalled();
  });
});
