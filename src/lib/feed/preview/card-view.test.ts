import { describe, expect, it } from "vitest";

import { cardView, CardViewKind } from "@/lib/feed/preview/card-view";
import { PreviewPane } from "@/lib/feed/preview/feed-previews";
import { FeedVideoKind } from "@/lib/types/feed";

const NOTHING_CACHED = {
  pane: PreviewPane.CODE,
  imageUrl: undefined,
  video: undefined,
  documentSource: undefined,
  lineCount: 0,
  isErrored: false
};

describe("cardView for an image", () => {
  it("shows the picture once it is cached", () => {
    expect(
      cardView({
        ...NOTHING_CACHED,
        path: "logo.png",
        imageUrl: "data:image/png;base64,AA"
      })
    ).toEqual({
      kind: CardViewKind.IMAGE,
      url: "data:image/png;base64,AA"
    });
  });

  it("says why there is no picture", () => {
    expect(
      cardView({
        ...NOTHING_CACHED,
        path: "logo.png",
        imageUrl: null
      })
    ).toEqual({
      kind: CardViewKind.MESSAGE,
      text: "No image preview available"
    });
    expect(
      cardView({
        ...NOTHING_CACHED,
        path: "logo.png",
        imageUrl: null,
        isErrored: true
      })
    ).toEqual({
      kind: CardViewKind.MESSAGE,
      text: "Couldn't load the image"
    });
  });
});

describe("cardView for a video", () => {
  it("transcodes until the clip lands", () => {
    expect(
      cardView({
        ...NOTHING_CACHED,
        path: "demo.mp4"
      })
    ).toEqual({
      kind: CardViewKind.LOADING,
      text: "Preparing the video preview"
    });
  });

  it("plays a ready clip", () => {
    expect(
      cardView({
        ...NOTHING_CACHED,
        path: "demo.mp4",
        video: {
          kind: FeedVideoKind.enum.ready,
          dataUrl: "data:video/mp4;base64,AA"
        }
      })
    ).toEqual({
      kind: CardViewKind.VIDEO,
      url: "data:video/mp4;base64,AA"
    });
  });

  it("asks for ffmpeg without a transcoder", () => {
    expect(
      cardView({
        ...NOTHING_CACHED,
        path: "demo.mp4",
        video: {
          kind: FeedVideoKind.enum.noTranscoder
        }
      })
    ).toEqual({
      kind: CardViewKind.MESSAGE,
      text: "Install ffmpeg to preview videos"
    });
  });

  it("tells a failed transcode from a missing clip", () => {
    expect(
      cardView({
        ...NOTHING_CACHED,
        path: "demo.mp4",
        video: null,
        isErrored: true
      })
    ).toEqual({
      kind: CardViewKind.MESSAGE,
      text: "Couldn't load the video"
    });
    expect(
      cardView({
        ...NOTHING_CACHED,
        path: "demo.mp4",
        video: null
      })
    ).toEqual({
      kind: CardViewKind.MESSAGE,
      text: "No video preview available"
    });
  });
});

describe("cardView for a document", () => {
  it("shows the diff while the Code pane is chosen", () => {
    expect(
      cardView({
        ...NOTHING_CACHED,
        path: "README.md",
        lineCount: 3
      })
    ).toEqual({ kind: CardViewKind.DIFF });
  });

  it("renders the document in the Preview pane", () => {
    expect(
      cardView({
        ...NOTHING_CACHED,
        path: "README.md",
        pane: PreviewPane.PREVIEW
      })
    ).toEqual({
      kind: CardViewKind.LOADING,
      text: "Rendering the preview"
    });
    expect(
      cardView({
        ...NOTHING_CACHED,
        path: "index.html",
        pane: PreviewPane.PREVIEW,
        documentSource: "<p>hi</p>"
      })
    ).toEqual({
      kind: CardViewKind.DOCUMENT,
      source: "<p>hi</p>"
    });
  });

  it("says why there is no rendered document", () => {
    expect(
      cardView({
        ...NOTHING_CACHED,
        path: "README.md",
        pane: PreviewPane.PREVIEW,
        documentSource: null,
        isErrored: true
      })
    ).toEqual({
      kind: CardViewKind.MESSAGE,
      text: "Couldn't render the preview"
    });
  });
});

describe("cardView for a text file", () => {
  it("shows its diff", () => {
    expect(
      cardView({
        ...NOTHING_CACHED,
        path: "main.ts",
        lineCount: 2
      })
    ).toEqual({ kind: CardViewKind.DIFF });
  });

  it("says why there is no diff", () => {
    expect(
      cardView({
        ...NOTHING_CACHED,
        path: "main.ts"
      })
    ).toEqual({
      kind: CardViewKind.MESSAGE,
      text: "No preview available"
    });
    expect(
      cardView({
        ...NOTHING_CACHED,
        path: "main.ts",
        isErrored: true
      })
    ).toEqual({
      kind: CardViewKind.MESSAGE,
      text: "Couldn't load a preview"
    });
  });
});
