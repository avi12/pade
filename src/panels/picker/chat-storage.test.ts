import { CHAT_SIZE_LIMIT_CHOICES, CHAT_STORAGE_LIMIT_CHOICES } from "@/lib/types";
import type { ChatStorage } from "@/lib/types";
import {
  agentShares,
  CHAT_SIZE_CHOICES,
  CHAT_STORAGE_CHOICES,
  cleanupSummary,
  keptAboveLimit,
  storageSummary
} from "@/panels/picker/chat-storage";
import { describe, expect, it } from "vitest";

const MEGABYTE = 1024 * 1024;
const GIGABYTE = 1024 * MEGABYTE;

const STORAGE: ChatStorage = {
  bytes: 13 * GIGABYTE,
  chatCount: 1234,
  agents: [
    {
      agentId: "claude",
      bytes: 12.5 * GIGABYTE,
      chatCount: 1000
    },
    {
      agentId: "codex",
      bytes: 139 * MEGABYTE,
      chatCount: 234
    }
  ],
  cleanup: {
    bytes: 10.8 * GIGABYTE,
    chatCount: 412
  }
};

describe("chat storage choices", () => {
  it("offers every limit, Off first, in megabytes or gigabytes", () => {
    expect(CHAT_SIZE_CHOICES.map(choice => choice.value)).toEqual([...CHAT_SIZE_LIMIT_CHOICES]);
    expect(CHAT_STORAGE_CHOICES.map(choice => choice.value)).toEqual([...CHAT_STORAGE_LIMIT_CHOICES]);
    expect(CHAT_SIZE_CHOICES.map(choice => choice.label)).toEqual(["Off", "100 MB", "200 MB", "500 MB"]);
    expect(CHAT_STORAGE_CHOICES.map(choice => choice.label)).toEqual(["Off", "1 GB", "2 GB", "5 GB", "10 GB"]);
  });
});

describe("chat storage wording", () => {
  it("sums the chats up", () => {
    expect(storageSummary(STORAGE)).toBe("13 GB in 1,234 chats");
  });

  it("names each agent by its label, an unknown one by its id", () => {
    expect(
      agentShares({
        storage: STORAGE,
        agents: [{
          id: "claude",
          label: "Claude Code",
          command: "claude",
          reordersBidi: true
        }]
      })
    ).toBe("Claude Code 12.5 GB · codex 139 MB");
  });

  it("says why the chats stay over the total only when they do", () => {
    expect(
      keptAboveLimit({
        storage: STORAGE,
        storageLimitMegabytes: 2048
      })
    ).toBe("Recent chats, and each project's newest, still take 2.2 GB, so the total stays above 2 GB until they're a week old");
    expect(
      keptAboveLimit({
        storage: STORAGE,
        storageLimitMegabytes: 5120
      })
    ).toBeNull();
    expect(
      keptAboveLimit({
        storage: STORAGE,
        storageLimitMegabytes: null
      })
    ).toBeNull();
  });

  it("says what a limit deletes", () => {
    expect(cleanupSummary(STORAGE.cleanup)).toBe("412 chats, 10.8 GB");
  });
});
