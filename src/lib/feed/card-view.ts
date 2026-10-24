// What an open Change Feed card shows below its bar: the picture, the clip, the
// rendered document or the diff - or the one line saying why there is none
// (yet). Pure, so every state a card can land in is tested rather than eyeballed.

import { PreviewPane } from "@/lib/feed/feed-previews";
import { isImagePath, isRenderablePath, isVideoPath } from "@/lib/feed/preview";
import { FeedVideoKind } from "@/lib/types";
import type { FeedVideo } from "@/lib/types";

export const CardViewKind = {
  Message: "message",
  Loading: "loading",
  Image: "image",
  Video: "video",
  Document: "document",
  Diff: "diff"
} as const;

/** A card whose preview is its picture or clip. */
export interface MediaView {
  kind: typeof CardViewKind.Image | typeof CardViewKind.Video;
  url: string;
}

type CardView =
  | {
    kind: typeof CardViewKind.Message | typeof CardViewKind.Loading;
    text: string;
  }
  | MediaView
  | {
    kind: typeof CardViewKind.Document;
    source: string;
  }
  | { kind: typeof CardViewKind.Diff };

/** A card's cached previews, `undefined` while a fetch has not landed. */
interface CardPreviews {
  path: string;
  pane: PreviewPane;
  imageUrl: string | null | undefined;
  video: FeedVideo | null | undefined;
  documentSource: string | null | undefined;
  lineCount: number;
  /** The card's last fetch failed, so an empty preview reads as an error. */
  isErrored: boolean;
}

function message(text: string) {
  return {
    kind: CardViewKind.Message,
    text
  } satisfies CardView;
}

// A preview still on its way, drawn with a loading animation.
function loading(text: string) {
  return {
    kind: CardViewKind.Loading,
    text
  } satisfies CardView;
}

// An empty preview is either the backend having nothing to show or the fetch
// having failed - the reader is told which.
function emptyMessage({ isErrored, failed, empty }: {
  isErrored: boolean;
  failed: string;
  empty: string;
}) {
  return message(isErrored ? failed : empty);
}

function imageView({ imageUrl, isErrored }: CardPreviews) {
  if (imageUrl) {
    return {
      kind: CardViewKind.Image,
      url: imageUrl
    } satisfies CardView;
  }

  return emptyMessage({
    isErrored,
    failed: "Couldn't load the image",
    empty: "No image preview available"
  });
}

function videoView({ video, isErrored }: CardPreviews) {
  if (video === undefined) {
    return loading("Preparing the video preview");
  }

  if (video?.kind === FeedVideoKind.enum.ready) {
    return {
      kind: CardViewKind.Video,
      url: video.dataUrl
    } satisfies CardView;
  }

  if (video?.kind === FeedVideoKind.enum.noTranscoder) {
    return message("Install ffmpeg to preview videos");
  }

  return emptyMessage({
    isErrored,
    failed: "Couldn't load the video",
    empty: "No video preview available"
  });
}

function documentView({ documentSource, isErrored }: CardPreviews) {
  if (documentSource === undefined) {
    return loading("Rendering the preview");
  }

  if (documentSource) {
    return {
      kind: CardViewKind.Document,
      source: documentSource
    } satisfies CardView;
  }

  return emptyMessage({
    isErrored,
    failed: "Couldn't render the preview",
    empty: "No preview available"
  });
}

function diffView({ lineCount, isErrored }: CardPreviews) {
  if (lineCount > 0) {
    return { kind: CardViewKind.Diff } satisfies CardView;
  }

  return emptyMessage({
    isErrored,
    failed: "Couldn't load a preview",
    empty: "No preview available"
  });
}

/** The view for an open card: media by its file type, a markdown/HTML file's
 *  rendered document while its Preview pane is chosen, else its diff. */
export function cardView(card: CardPreviews) {
  if (isImagePath(card.path)) {
    return imageView(card);
  }

  if (isVideoPath(card.path)) {
    return videoView(card);
  }

  const isPreviewShown = isRenderablePath(card.path) && card.pane === PreviewPane.Preview;
  if (isPreviewShown) {
    return documentView(card);
  }

  return diffView(card);
}
