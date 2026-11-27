import { beforeEach, describe, expect, it, vi } from "vitest";

import type * as messagingModule from "@/lib/ipc/messaging";
import { observeRunningModel } from "@/lib/stores/agent/context.svelte";
import { adaptedChoice, adaptedSteps, createUsageAdapter } from "@/lib/stores/usage/usageAdapt.svelte";

vi.mock("@/lib/ipc/platform", () => ({
  windowLabel: () => "main"
}));
vi.mock("@/lib/ipc/messaging", async importOriginal => ({
  ...(await importOriginal<typeof messagingModule>()),
  sendMessage: vi.fn(async () => null)
}));

const stored = new Map<string, string>();
vi.stubGlobal("localStorage", {
  getItem: (key: string) => stored.get(key) ?? null,
  setItem: (key: string, value: string) => stored.set(key, value),
  removeItem: (key: string) => stored.delete(key)
});

const SESSION = "paced-then-switched";

/** Pacing's stored record: Sonnet medium stepped down to Sonnet low. */
function storeRecord() {
  localStorage.setItem(
    "pade.usage-adapt:main",
    JSON.stringify({
      [SESSION]: {
        trail: [
          {
            step: "effort",
            previous: {
              model: "claude-sonnet-5-5",
              effort: "medium"
            }
          }
        ],
        current: {
          model: "claude-sonnet-5-5",
          effort: "low"
        },
        restoreAt: Date.now() + 60_000
      }
    })
  );
  createUsageAdapter({
    async switchModel() {},
    announce() {}
  });
}

describe("a pacing record the session has outrun", () => {
  beforeEach(storeRecord);

  it("still shows while the session runs what pacing set", () => {
    observeRunningModel({
      id: SESSION,
      model: "claude-sonnet-5-5",
      effort: "low",
      isUltracode: false
    });
    expect(adaptedSteps(SESSION)).toEqual(["effort"]);
  });

  it("shows no more once the session runs something else", () => {
    observeRunningModel({
      id: SESSION,
      model: "claude-opus-5-5",
      effort: "xhigh",
      isUltracode: false
    });
    expect(adaptedSteps(SESSION)).toEqual([]);
    expect(adaptedChoice(SESSION)).toBeNull();
  });
});
