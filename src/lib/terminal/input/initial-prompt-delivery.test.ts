import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import { createInitialPromptDelivery } from "@/lib/terminal/input/initial-prompt-delivery";
import { pastedText } from "@/lib/terminal/input/terminal-input";
import { CURSOR_DOWN, ENTER } from "@/lib/terminal/input/terminal-sequences";

const PROMPT = "summarise the repository layout";
const GATE_ON_NO = [" ❯ No, exit", "   Yes, I trust this folder"];
const REPL = [' ❯  Try "write a test for <filepath>"', " ⏵⏵ bypass permissions on"];

function deliveryFor({
  prompt = PROMPT,
  isWorking = false
}: {
  prompt?: string;
  isWorking?: boolean;
} = {}) {
  const written: string[] = [];
  const state = {
    isWorking,
    isSettled: false
  };
  const delivery = createInitialPromptDelivery({
    prompt: () => prompt,
    async write(data) {
      written.push(data);
    },
    isWorking: () => state.isWorking,
    settle() {
      state.isSettled = true;
    }
  });
  return {
    delivery,
    written,
    state
  };
}

describe("createInitialPromptDelivery", () => {
  beforeEach(() => {
    vi.useFakeTimers();
  });

  afterEach(() => {
    vi.useRealTimers();
  });

  it("walks the trust gate one key per repaint", async () => {
    const { delivery, written } = deliveryFor();
    await delivery.answerTrustGate(GATE_ON_NO);
    await delivery.answerTrustGate(GATE_ON_NO);
    expect(written).toEqual([CURSOR_DOWN]);

    delivery.watchOutput("repaint");
    await delivery.answerTrustGate(GATE_ON_NO);
    expect(written).toEqual([CURSOR_DOWN, CURSOR_DOWN]);
  });

  it("holds the prompt while the gate is on screen", async () => {
    const { delivery, written } = deliveryFor();
    await delivery.answerTrustGate(GATE_ON_NO);
    await delivery.deliverIfReady();
    expect(written).toEqual([CURSOR_DOWN]);
  });

  it("delivers once the gate leaves the screen, even after the agent already fell quiet", async () => {
    const { delivery, written } = deliveryFor();
    await delivery.answerTrustGate(GATE_ON_NO);
    // The agent fell quiet while the last sample still showed the gate.
    await delivery.deliverIfReady();
    expect(written).toEqual([CURSOR_DOWN]);

    // A later sample - drawn late - finds the gate gone: deliver now, since no
    // further quiet moment is coming to do it.
    delivery.watchOutput("repaint");
    const delivering = delivery.answerTrustGate(REPL);
    await vi.advanceTimersByTimeAsync(1_200);
    await delivering;
    expect(written).toEqual([CURSOR_DOWN, pastedText(PROMPT), ENTER]);
  });

  it("leaves a still-working agent's prompt to its own quiet moment", async () => {
    const { delivery, written } = deliveryFor({ isWorking: true });
    await delivery.answerTrustGate(GATE_ON_NO);
    delivery.watchOutput("repaint");
    await delivery.answerTrustGate(REPL);
    expect(written).toEqual([CURSOR_DOWN]);
  });

  it("pastes, settles, submits, and latches once the agent is seen working", async () => {
    const { delivery, written, state } = deliveryFor();
    const delivering = delivery.deliverIfReady();
    await vi.advanceTimersByTimeAsync(1_200);
    await delivering;
    expect(written).toEqual([pastedText(PROMPT), ENTER]);

    expect(state.isSettled).toBe(false);
    state.isWorking = true;
    await vi.advanceTimersByTimeAsync(2_000);
    await delivery.deliverIfReady();
    expect(written).toEqual([pastedText(PROMPT), ENTER]);
    // Settled, so the prompt no longer rides along on a reload or a hand-off.
    expect(state.isSettled).toBe(true);
  });

  it("re-sends only the Enter when the paste already echoed", async () => {
    const { delivery, written } = deliveryFor();
    const delivering = delivery.deliverIfReady();
    await vi.advanceTimersByTimeAsync(1_200);
    await delivering;
    delivery.watchOutput(PROMPT);

    await vi.advanceTimersByTimeAsync(2_000);
    expect(written).toEqual([pastedText(PROMPT), ENTER, ENTER]);
  });

  it("does nothing for a session without a first prompt", async () => {
    const { delivery, written } = deliveryFor({ prompt: "" });
    await delivery.answerTrustGate(GATE_ON_NO);
    await delivery.deliverIfReady();
    expect(written).toEqual([]);
  });
});
