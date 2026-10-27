import { createFeedPreviews, PreviewPane } from "@/lib/feed/preview/feed-previews";
import { Command } from "@/lib/ipc/messaging";
import {
  beforeEach,
  describe,
  expect,
  it,
  vi
} from "vitest";

const mocks = vi.hoisted(() => ({
  sendMessage: vi.fn<(command: string, data?: unknown) => Promise<unknown>>()
}));

vi.mock("@/lib/ipc/messaging", async importOriginal => ({
  ...await importOriginal<typeof import("@/lib/ipc/messaging")>(),
  sendMessage: mocks.sendMessage
}));

describe("feed previews", () => {
  beforeEach(() => {
    mocks.sendMessage.mockReset();
  });

  it("fetches a card's diff once and caches it by event id", async () => {
    mocks.sendMessage.mockResolvedValue({
      before: "a\n",
      after: "b\n"
    });
    const previews = createFeedPreviews();
    const card = {
      id: "e1",
      path: "src/a.ts"
    };

    await previews.loadCardPreview(card);
    await previews.loadCardPreview(card);

    expect(mocks.sendMessage).toHaveBeenCalledTimes(1);
    expect(mocks.sendMessage).toHaveBeenCalledWith(Command.feedDiff, { path: "src/a.ts" });
    expect(previews.diffCache.get("e1")?.length).toBeGreaterThan(0);
  });

  it("records a failed fetch instead of rejecting", async () => {
    mocks.sendMessage.mockRejectedValue(new Error("gone"));
    const previews = createFeedPreviews();

    await previews.loadCardPreview({
      id: "e2",
      path: "logo.png"
    });

    expect(mocks.sendMessage).toHaveBeenCalledWith(Command.feedImage, { path: "logo.png" });
    expect(previews.failedIds.has("e2")).toBe(true);
    expect(previews.imageCache.get("e2")).toBeNull();
  });

  it("renders a markdown card's document for its Preview pane", async () => {
    mocks.sendMessage.mockResolvedValue("# Title");
    const previews = createFeedPreviews();

    await previews.loadPreviewDocument({
      id: "e3",
      path: "README.md"
    });

    expect(mocks.sendMessage).toHaveBeenCalledWith(Command.feedText, { path: "README.md" });
    expect(previews.documentCache.get("e3")).toContain("Title");
  });

  it("reads every preview through a given source instead of the watch", async () => {
    const source = {
      diff: vi.fn(async () => []),
      image: vi.fn(async () => ({ dataUrl: "data:image/png;base64,AA==" })),
      video: vi.fn(async () => null),
      text: vi.fn(async () => "# Committed")
    };
    const previews = createFeedPreviews(source);
    const card = {
      id: "logo.png",
      path: "logo.png"
    };

    expect(previews.isCardPreviewLoaded(card)).toBe(false);
    await previews.loadCardPreview(card);
    await previews.loadPreviewDocument({
      id: "README.md",
      path: "README.md"
    });

    expect(previews.isCardPreviewLoaded(card)).toBe(true);
    expect(previews.imageCache.get("logo.png")).toBe("data:image/png;base64,AA==");
    expect(previews.documentCache.get("README.md")).toContain("Committed");
    expect(mocks.sendMessage).not.toHaveBeenCalled();
  });

  it("shows the Code pane until a card is switched", () => {
    const previews = createFeedPreviews();
    expect(previews.paneOf("e4")).toBe(PreviewPane.Code);

    previews.paneById.set("e4", PreviewPane.Preview);
    expect(previews.paneOf("e4")).toBe(PreviewPane.Preview);
  });
});
