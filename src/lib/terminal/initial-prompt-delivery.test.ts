import { createInitialPromptDelivery } from "@/lib/terminal/initial-prompt-delivery";
import { pastedText } from "@/lib/terminal/terminal-input";
import { CURSOR_DOWN, ENTER } from "@/lib/terminal/terminal-sequences";
import {
  afterEach,
  beforeEach,
  describe,
  expect,
  it,
  vi
} from "vitest";

const PROMPT = "summarise the repository layout";
const GATE_ON_NO = [" ❯ No, exit", "   Yes, I trust this folder"];

function deliveryFor({ prompt = PROMPT, isWorking = false }: {
  prompt?: string;
  isWorking?: boolean;
} = {}) {
  const written: string[] = [];
  const state = { isWorking };
  const delivery = createInitialPromptDelivery({
    prompt: () => prompt,
    async write(data) {
      written.push(data);
    },
    isWorking: () => state.isWorking
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

  it("pastes, settles, submits, and latches once the agent is seen working", async () => {
    const { delivery, written, state } = deliveryFor();
    const delivering = delivery.deliverIfReady();
    await vi.advanceTimersByTimeAsync(1_200);
    await delivering;
    expect(written).toEqual([pastedText(PROMPT), ENTER]);

    state.isWorking = true;
    await vi.advanceTimersByTimeAsync(2_000);
    await delivery.deliverIfReady();
    expect(written).toEqual([pastedText(PROMPT), ENTER]);
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
