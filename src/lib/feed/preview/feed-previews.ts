// The lazily fetched card previews - the diff, picture, clip or rendered
// document each card shows when opened - cached per card id for the life of one
// mounted view. The Change Feed keys them by event id (not path) so repeated
// edits to the same file - distinct events sharing a path - each render their
// own preview rather than the stale first one; the commit view, reading the same
// previews out of a commit through its own source, keys them by path. Every
// fetch runs on a card's first open only, so a view full of images or docs never
// fetches them all at once.

import type { DiffLine } from "@/lib/code/diff";
import { markdownDocument, sandboxedHtmlDocument } from "@/lib/code/markdown";
import { isImagePath, isMarkdownPath, isVideoPath } from "@/lib/feed/preview/preview";
import { previewLines } from "@/lib/feed/preview/preview-lines";
import { Command, sendMessage } from "@/lib/ipc/messaging";
import type { FeedImage, FeedVideo } from "@/lib/types";
import { SvelteMap, SvelteSet } from "svelte/reactivity";

/** A markdown/HTML card shows a Code|Preview toggle: Code is the diff (the
 *  feed's core purpose, so the default), Preview is the rendered result in a
 *  sandboxed iframe. */
export const PreviewPane = {
  Code: "code",
  Preview: "preview"
} as const;
export type PreviewPane = (typeof PreviewPane)[keyof typeof PreviewPane];

// A video card usually opens while the agent is still writing the file, and
// ffmpeg can't read a half-written MP4 (its index lands last). So an empty
// transcode is retried a few times before the card gives up on it.
const VIDEO_TRANSCODE_ATTEMPTS = 5;
const VIDEO_RETRY_DELAY_MS = 1500;

function delay(milliseconds: number) {
  return new Promise(resolve => {
    setTimeout(resolve, milliseconds);
  });
}

async function transcodeVideo(path: string) {
  for (let attempt = 1; attempt < VIDEO_TRANSCODE_ATTEMPTS; attempt += 1) {
    const video = await sendMessage(Command.feedVideo, { path });
    if (video) {
      return video;
    }

    await delay(VIDEO_RETRY_DELAY_MS);
  }

  return sendMessage(Command.feedVideo, { path });
}

/** One card whose previews are cached under `id`. */
export interface PreviewCard {
  id: string;
  path: string;
}

/** Where a card's previews are read from, one fetch per preview kind. */
export interface PreviewSource {
  diff: (path: string) => Promise<DiffLine[]>;
  image: (path: string) => Promise<FeedImage | null>;
  video: (path: string) => Promise<FeedVideo | null>;
  text: (path: string) => Promise<string | null>;
}

// The Change Feed's source: the file as it is now in the watched project.
// Git-free diff: the backend hands over the session baseline and the current
// content; the shared parse+render path draws the diff (or the whole file when
// the baseline landed late - see previewLines).
const FEED_SOURCE: PreviewSource = {
  diff: async path => previewLines(await sendMessage(Command.feedDiff, { path })),
  image: path => sendMessage(Command.feedImage, { path }),
  video: transcodeVideo,
  text: path => sendMessage(Command.feedText, { path })
};

// The iframe srcdoc for a file's current text: markdown is rendered to a full
// themed document, an HTML file is shown as-is (both inert - see
// SandboxedPreview).
function previewDocument({ path, text }: {
  path: string;
  text: string;
}) {
  return isMarkdownPath(path) ? markdownDocument(text) : sandboxedHtmlDocument(text);
}

export function createFeedPreviews(source = FEED_SOURCE) {
  // The parsed diff lines per card, or `null` when the fetch failed.
  const diffCache = new SvelteMap<string, DiffLine[] | null>();
  // The data URL for an image card, or `null` when the backend has nothing to
  // show (gone, or over its size cap).
  const imageCache = new SvelteMap<string, string | null>();
  // Transcoded clips. A transcode takes a moment, so a video card expands at
  // once and shows a loading state until its entry lands.
  const videoCache = new SvelteMap<string, FeedVideo | null>();
  // The sandbox-ready srcdoc (a full HTML document for markdown, the raw file
  // text for HTML), or `null` when its text couldn't be fetched.
  const documentCache = new SvelteMap<string, string | null>();
  // Ids whose fetch failed, so a re-opened previously-failed card shows
  // "Couldn't load" rather than the empty-cache "No preview" message.
  const failedIds = new SvelteSet<string>();
  // The pane each previewable card shows, Code until switched.
  const paneById = new SvelteMap<string, PreviewPane>();

  // Fill `cache` for `id` from `load` once, recording a failure instead of
  // rejecting - the shared lazy fetch behind every preview.
  async function loadOnce<Preview>({ cache, id, load }: {
    cache: SvelteMap<string, Preview | null>;
    id: string;
    load: () => Promise<Preview | null>;
  }) {
    if (cache.has(id)) {
      return;
    }

    try {
      cache.set(id, await load());
      failedIds.delete(id);
    } catch {
      failedIds.add(id);
      cache.set(id, null);
    }
  }

  /** Fill the preview cache for one card - the picture for an image, the clip
   *  for a video, else the baseline diff. Never rejects. */
  async function loadCardPreview({ id, path }: PreviewCard) {
    if (isImagePath(path)) {
      await loadOnce({
        cache: imageCache,
        id,
        load: async () => (await source.image(path))?.dataUrl ?? null
      });
      return;
    }

    if (isVideoPath(path)) {
      await loadOnce({
        cache: videoCache,
        id,
        load: () => source.video(path)
      });
      return;
    }

    await loadOnce({
      cache: diffCache,
      id,
      load: () => source.diff(path)
    });
  }

  /** Whether a card's `loadCardPreview` fetch has landed, failed or not. */
  function isCardPreviewLoaded({ id, path }: PreviewCard) {
    if (isImagePath(path)) {
      return imageCache.has(id);
    }

    if (isVideoPath(path)) {
      return videoCache.has(id);
    }

    return diffCache.has(id);
  }

  /** Fetch a markdown/HTML card's text and cache its srcdoc, on the first
   *  switch to Preview only. Never rejects. */
  async function loadPreviewDocument({ id, path }: PreviewCard) {
    if (documentCache.has(id)) {
      return;
    }

    try {
      const text = await source.text(path);
      // Nothing to render is not a failure - and not a success either, so an
      // earlier failure for this card still reads as one.
      if (text === null) {
        documentCache.set(id, null);
        return;
      }

      documentCache.set(
        id, previewDocument({
          path,
          text
        })
      );
      failedIds.delete(id);
    } catch {
      failedIds.add(id);
      documentCache.set(id, null);
    }
  }

  return {
    diffCache,
    imageCache,
    videoCache,
    documentCache,
    failedIds,
    paneById,
    loadCardPreview,
    isCardPreviewLoaded,
    loadPreviewDocument,
    paneOf: (id: string) => paneById.get(id) ?? PreviewPane.Code
  };
}

export type FeedPreviews = ReturnType<typeof createFeedPreviews>;
