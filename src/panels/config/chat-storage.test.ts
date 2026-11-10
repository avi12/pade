import { CHAT_SIZE_LIMIT_CHOICES, CHAT_STORAGE_LIMIT_CHOICES } from "@/lib/types";
import type { ChatStorage } from "@/lib/types";
import {
  agentShares,
  CHAT_SIZE_CHOICES,
  CHAT_STORAGE_CHOICES,
  cleanupSummary,
  deletedChatRow,
  keptAboveLimit,
  storageSummary,
  tickedCleanup,
  unlistedChats
} from "@/panels/config/chat-storage";
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
    chatCount: 412,
    chats: []
  }
};

const DAY = 24 * 60 * 60 * 1000;
const NOW = 100 * DAY;

const CLAUDE = {
  id: "claude",
  label: "Claude Code",
  command: "claude",
  reordersBidi: true
};

const listed = {
  id: "listed",
  agentId: "claude",
  title: null,
  project: null,
  bytes: 1,
  updatedAt: 0
};

describe("the deleted chats list", () => {
  it("names a chat by title, agent, folder, size and age", () => {
    expect(
      deletedChatRow({
        chat: {
          id: "fix-the-build",
          agentId: "claude",
          title: "Fix the build",
          project: String.raw`C:\repositories\avi\edge-plus`,
          bytes: 206 * MEGABYTE,
          updatedAt: NOW - 10 * DAY
        },
        agents: [CLAUDE],
        now: NOW
      })
    ).toEqual({
      title: "Fix the build",
      details: "Claude Code · edge-plus · 206 MB · 10d ago"
    });
  });

  it("lists an untitled chat from an unknown folder without gaps", () => {
    expect(
      deletedChatRow({
        chat: {
          id: "untitled",
          agentId: "codex",
          title: null,
          project: null,
          bytes: 3 * MEGABYTE,
          updatedAt: NOW - 2 * DAY
        },
        agents: [CLAUDE],
        now: NOW
      })
    ).toEqual({
      title: "Untitled chat",
      details: "codex · 3 MB · 2d ago"
    });
  });

  it("deletes only the ticked chats", () => {
    const cleanup = {
      bytes: 10 * MEGABYTE,
      chatCount: 5,
      chats: [
        {
          ...listed,
          id: "a",
          bytes: 4 * MEGABYTE
        },
        {
          ...listed,
          id: "b",
          bytes: 3 * MEGABYTE
        }
      ]
    };
    expect(
      tickedCleanup({
        cleanup,
        ticked: new Set(["a"])
      })
    ).toEqual({
      bytes: 4 * MEGABYTE,
      chatCount: 1
    });
    expect(
      cleanupSummary(
        tickedCleanup({
          cleanup,
          ticked: new Set(["a", "b"])
        })
      )
    ).toBe("2 chats, 7 MB");
    expect(
      tickedCleanup({
        cleanup,
        ticked: new Set()
      })
    ).toEqual({
      bytes: 0,
      chatCount: 0
    });
  });

  it("counts the chats it leaves out, only when it leaves some out", () => {
    expect(
      unlistedChats({
        bytes: 3,
        chatCount: 3,
        chats: [listed]
      })
    ).toBe("And 2 more over the limit, newer or smaller");
    expect(
      unlistedChats({
        bytes: 1,
        chatCount: 1,
        chats: [listed]
      })
    ).toBeNull();
  });
});

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
