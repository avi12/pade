import { z } from "zod";

export const ChangeKind = z.enum(["created", "modified", "deleted"]);

/** One change a watch surfaced. `root` is the held project root it belongs to,
 *  spelled exactly as the frontend sent it in `watch_projects`, so the feed
 *  files it under that project without re-deriving which one a path is in. */
export const ChangeEvent = z.object({
  id: z.string(),
  root: z.string(),
  path: z.string(),
  kind: ChangeKind,
  added: z.number(),
  removed: z.number(),
  summary: z.string(),
  ts: z.number()
});

export type ChangeEvent = z.infer<typeof ChangeEvent>;

/** The Change Feed's git-free preview inputs for one path: the session's
 *  first-touch baseline snapshot (`before`) and the file's current on-disk
 *  content (`after`, empty when it is now deleted). The whole payload is nullable
 *  at the boundary - `null` = nothing to preview (binary, over the size cap, or a
 *  path with no captured baseline) - so the card shows "No preview available". */
export const FeedDiff = z.object({
  before: z.string(),
  after: z.string()
});

export type FeedDiff = z.infer<typeof FeedDiff>;

/** What this window's Change Feed watch is doing: the root it is armed on
 *  (`null` = no live watch), when it armed, and how many changes it has
 *  surfaced versus dropped as ignored since. The feed holds nothing from before
 *  `armedAt` - the backend replays nothing - so the panel states all of it when
 *  the list is empty rather than leaving "nothing changed" and "nothing is
 *  watched" looking identical. */
export const WatchStatus = z.object({
  root: z.string().nullable(),
  armedAt: z.number().nullable(),
  surfaced: z.number(),
  ignored: z.number()
});

export type WatchStatus = z.infer<typeof WatchStatus>;

/** A watch event about one held project as a whole (its ignore rules, its git
 *  state): the root as the frontend spelled it in `watch_projects`. */
export const WatchedRootEvent = z.object({
  root: z.string()
});

export type WatchedRootEvent = z.infer<typeof WatchedRootEvent>;

/** A Change Feed image preview: the file's bytes as a ready-to-use `data:` URL,
 *  so the card renders it with a plain `<img src>`. The whole payload is nullable
 *  at the boundary (see `feed_image`). */
export const FeedImage = z.object({
  dataUrl: z.string()
});

export type FeedImage = z.infer<typeof FeedImage>;

/** How a Change Feed video preview turned out: a transcoded clip, or no clip
 *  because ffmpeg isn't installed. */
export const FeedVideoKind = z.enum(["ready", "noTranscoder"]);

/** A Change Feed video preview: an ffmpeg-transcoded clip as a `data:` URL for a
 *  plain `<video src>`, or word that ffmpeg is missing (see `feed.video`). */
export const FeedVideo = z.discriminatedUnion("kind", [
  z.object({
    kind: z.literal(FeedVideoKind.enum.ready),
    dataUrl: z.string()
  }),
  z.object({ kind: z.literal(FeedVideoKind.enum.noTranscoder) })
]);

export type FeedVideo = z.infer<typeof FeedVideo>;
