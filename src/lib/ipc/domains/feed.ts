import { z } from "zod";

import { NO_REPLY } from "@/lib/ipc/domains/no-reply";
import type { PathData } from "@/lib/ipc/domains/path-data";
import { ChangeEvent, FeedDiff, FeedImage, FeedVideo, WatchedRootEvent, WatchStatus } from "@/lib/types/feed";

export const FEED_COMMANDS = {
  WATCH_PROJECTS: "watch_projects",
  WATCH_STATUS: "watch_status",
  WATCH_DIRS: "watch_dirs",
  WATCH_LANGUAGES: "watch_languages",
  FEED_DIFF: "feed_diff",
  FEED_IMAGE: "feed_image",
  FEED_VIDEO: "feed_video",
  FEED_TEXT: "feed_text",
  FEED_IGNORED: "feed_ignored"
} as const;

export const FEED_EVENTS = {
  FEED_CHANGE: "feed://change",
  FEED_IGNORE_CHANGED: "feed://ignore-changed",
  DIRECTORIES_CHANGED: "directories://changed",
  LANGUAGES_CHANGED: "languages://changed"
} as const;

export interface FeedCommandProtocol {
  /** Watch every project the window holds a session of - the whole set, every
   *  call. Roots new to the set are armed, roots gone from it are dropped, the
   *  rest are left alone (their baselines survive); the newest call wins. */
  [FEED_COMMANDS.WATCH_PROJECTS]: (data: { roots: string[] }) => void;
  /** What this window's watch of `root` is doing right now, for the feed's
   *  empty panel and its reload snapshot; empty when `root` is not watched. */
  [FEED_COMMANDS.WATCH_STATUS]: (data: { root: string }) => WatchStatus;
  /** Arm the picker's folder watcher on the parents of the rows on the page. */
  [FEED_COMMANDS.WATCH_DIRS]: (data: { dirs: string[] }) => void;
  /** Watch the `.git` folders of the project rows on screen, so a row whose
   *  tracked files change re-counts its languages; an empty list stops. */
  [FEED_COMMANDS.WATCH_LANGUAGES]: (data: { paths: string[] }) => void;
  /** The card's git-free preview: the first-touch baseline vs current content. */
  [FEED_COMMANDS.FEED_DIFF]: (data: PathData) => FeedDiff | null;
  /** An image path's current bytes as a `data:` URL, `null` past the size cap. */
  [FEED_COMMANDS.FEED_IMAGE]: (data: PathData) => FeedImage | null;
  /** A video path's opening seconds transcoded to an MP4 `data:` URL, or
   *  `noTranscoder` without ffmpeg. */
  [FEED_COMMANDS.FEED_VIDEO]: (data: PathData) => FeedVideo | null;
  /** The current text of a watched markdown/HTML path, for the card's Preview. */
  [FEED_COMMANDS.FEED_TEXT]: (data: PathData) => string | null;
  /** The subset of `paths` the current ignore policy excludes. */
  [FEED_COMMANDS.FEED_IGNORED]: (data: { paths: string[] }) => string[];
}

export const FEED_RESPONSE_SCHEMAS = {
  [FEED_COMMANDS.WATCH_PROJECTS]: NO_REPLY,
  [FEED_COMMANDS.WATCH_STATUS]: WatchStatus,
  [FEED_COMMANDS.WATCH_DIRS]: NO_REPLY,
  [FEED_COMMANDS.WATCH_LANGUAGES]: NO_REPLY,
  [FEED_COMMANDS.FEED_DIFF]: FeedDiff.nullable(),
  [FEED_COMMANDS.FEED_IMAGE]: FeedImage.nullable(),
  [FEED_COMMANDS.FEED_VIDEO]: FeedVideo.nullable(),
  [FEED_COMMANDS.FEED_TEXT]: z.string().nullable(),
  [FEED_COMMANDS.FEED_IGNORED]: z.array(z.string())
} satisfies { [Key in keyof FeedCommandProtocol]: z.ZodType<ReturnType<FeedCommandProtocol[Key]>> };

export interface FeedEventProtocol {
  [FEED_EVENTS.FEED_CHANGE]: (data: ChangeEvent) => void;
  /** A held project's ignore rules changed (a `.gitignore` edit, a mid-session
   *  `git init`) - re-ask `feed_ignored` about that project's events. */
  [FEED_EVENTS.FEED_IGNORE_CHANGED]: (data: WatchedRootEvent) => void;
  /** A picker-watched folder gained or lost a child (a project created or
   *  deleted outside PADE). */
  [FEED_EVENTS.DIRECTORIES_CHANGED]: (data: null) => void;
  /** A watched project row's tracked files changed - its path, as sent. */
  [FEED_EVENTS.LANGUAGES_CHANGED]: (data: string) => void;
}

export const FEED_PAYLOAD_SCHEMAS = {
  [FEED_EVENTS.FEED_CHANGE]: ChangeEvent,
  [FEED_EVENTS.FEED_IGNORE_CHANGED]: WatchedRootEvent,
  [FEED_EVENTS.DIRECTORIES_CHANGED]: z.null(),
  [FEED_EVENTS.LANGUAGES_CHANGED]: z.string()
} satisfies { [Key in keyof FeedEventProtocol]: z.ZodType<Parameters<FeedEventProtocol[Key]>[0]> };
