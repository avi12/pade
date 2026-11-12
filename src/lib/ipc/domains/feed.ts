import { NO_REPLY } from "@/lib/ipc/domains/no-reply";
import type { PathData } from "@/lib/ipc/domains/path-data";
import {
  ChangeEvent,
  FeedDiff,
  FeedImage,
  FeedVideo,
  WatchedRootEvent,
  WatchStatus
} from "@/lib/types/feed";
import { z } from "zod";

export const FEED_COMMANDS = {
  watchProjects: "watch_projects",
  watchStatus: "watch_status",
  watchDirs: "watch_dirs",
  watchLanguages: "watch_languages",
  feedDiff: "feed_diff",
  feedImage: "feed_image",
  feedVideo: "feed_video",
  feedText: "feed_text",
  feedIgnored: "feed_ignored"
} as const;

export const FEED_EVENTS = {
  feedChange: "feed://change",
  feedIgnoreChanged: "feed://ignore-changed",
  directoriesChanged: "directories://changed",
  languagesChanged: "languages://changed"
} as const;

export interface FeedCommandProtocol {
  /** Watch every project the window holds a session of - the whole set, every
   *  call. Roots new to the set are armed, roots gone from it are dropped, the
   *  rest are left alone (their baselines survive); the newest call wins. */
  [FEED_COMMANDS.watchProjects]: (data: { roots: string[] }) => void;
  /** What this window's watch of `root` is doing right now, for the feed's
   *  empty panel and its reload snapshot; empty when `root` is not watched. */
  [FEED_COMMANDS.watchStatus]: (data: { root: string }) => WatchStatus;
  /** Arm the picker's folder watcher on the parents of the rows on the page. */
  [FEED_COMMANDS.watchDirs]: (data: { dirs: string[] }) => void;
  /** Watch the `.git` folders of the project rows on screen, so a row whose
   *  tracked files change re-counts its languages; an empty list stops. */
  [FEED_COMMANDS.watchLanguages]: (data: { paths: string[] }) => void;
  /** The card's git-free preview: the first-touch baseline vs current content. */
  [FEED_COMMANDS.feedDiff]: (data: PathData) => FeedDiff | null;
  /** An image path's current bytes as a `data:` URL, `null` past the size cap. */
  [FEED_COMMANDS.feedImage]: (data: PathData) => FeedImage | null;
  /** A video path's opening seconds transcoded to an MP4 `data:` URL, or
   *  `noTranscoder` without ffmpeg. */
  [FEED_COMMANDS.feedVideo]: (data: PathData) => FeedVideo | null;
  /** The current text of a watched markdown/HTML path, for the card's Preview. */
  [FEED_COMMANDS.feedText]: (data: PathData) => string | null;
  /** The subset of `paths` the current ignore policy excludes. */
  [FEED_COMMANDS.feedIgnored]: (data: { paths: string[] }) => string[];
}

export const FEED_RESPONSE_SCHEMAS = {
  [FEED_COMMANDS.watchProjects]: NO_REPLY,
  [FEED_COMMANDS.watchStatus]: WatchStatus,
  [FEED_COMMANDS.watchDirs]: NO_REPLY,
  [FEED_COMMANDS.watchLanguages]: NO_REPLY,
  [FEED_COMMANDS.feedDiff]: FeedDiff.nullable(),
  [FEED_COMMANDS.feedImage]: FeedImage.nullable(),
  [FEED_COMMANDS.feedVideo]: FeedVideo.nullable(),
  [FEED_COMMANDS.feedText]: z.string().nullable(),
  [FEED_COMMANDS.feedIgnored]: z.array(z.string())
} satisfies { [Key in keyof FeedCommandProtocol]: z.ZodType<ReturnType<FeedCommandProtocol[Key]>> };

export interface FeedEventProtocol {
  [FEED_EVENTS.feedChange]: (data: ChangeEvent) => void;
  /** A held project's ignore rules changed (a `.gitignore` edit, a mid-session
   *  `git init`) - re-ask `feed_ignored` about that project's events. */
  [FEED_EVENTS.feedIgnoreChanged]: (data: WatchedRootEvent) => void;
  /** A picker-watched folder gained or lost a child (a project created or
   *  deleted outside PADE). */
  [FEED_EVENTS.directoriesChanged]: (data: null) => void;
  /** A watched project row's tracked files changed - its path, as sent. */
  [FEED_EVENTS.languagesChanged]: (data: string) => void;
}

export const FEED_PAYLOAD_SCHEMAS = {
  [FEED_EVENTS.feedChange]: ChangeEvent,
  [FEED_EVENTS.feedIgnoreChanged]: WatchedRootEvent,
  [FEED_EVENTS.directoriesChanged]: z.null(),
  [FEED_EVENTS.languagesChanged]: z.string()
} satisfies { [Key in keyof FeedEventProtocol]: z.ZodType<Parameters<FeedEventProtocol[Key]>[0]> };
