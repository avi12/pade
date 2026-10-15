// Which Change Feed changes are previewable, and as what, decided by file
// extension. The single authoritative TS home for every previewable-file
// extension set - images (rendered inline via `<img src>`), videos (an
// ffmpeg-transcoded clip in a `<video>`), markdown (rendered
// to HTML), and HTML (rendered as-is) - so the classifiers can never disagree
// about which renderer a card routes to. The image list mirrors the backend's
// IMAGE_MIME_TYPES (watcher.rs), where each extension is also mapped to its MIME
// type; the two lists are kept in sync by hand.

import { pathExtension } from "@/lib/feed/file-type";
import { z } from "zod";

/** The image file extensions the Change Feed previews inline (lower-case, no
 *  leading dot). SVG is included - it is rendered through `<img src>` like every
 *  other image, never inlined as markup, so it can carry no active content. */
const ImageExtension = z.enum([
  "png",
  "jpg",
  "jpeg",
  "gif",
  "webp",
  "avif",
  "bmp",
  "ico",
  "svg"
]);

/** The video file extensions the Change Feed previews as a transcoded clip. Its
 *  backend mirror is VIDEO_EXTENSIONS (media.rs), kept in sync by hand. */
const VideoExtension = z.enum([
  "mp4",
  "m4v",
  "mov",
  "webm",
  "mkv",
  "avi",
  "wmv",
  "flv",
  "mpg",
  "mpeg",
  "ogv",
  "3gp"
]);

/** The markdown extensions the Change Feed can render to a preview. */
const MarkdownExtension = z.enum(["md", "markdown"]);

/** The HTML extensions the Change Feed can render (inertly) as a preview. */
const HtmlExtension = z.enum(["html", "htm"]);

/** Binary file extensions a text editor cannot usefully open - executables,
 *  libraries, archives, office documents, fonts, audio and databases. The feed
 *  reveals these in the file manager instead of handing them to the editor. */
const BinaryExtension = z.enum([
  "exe",
  "dll",
  "msi",
  "sys",
  "so",
  "dylib",
  "bin",
  "o",
  "obj",
  "lib",
  "a",
  "pdb",
  "class",
  "jar",
  "war",
  "pyc",
  "wasm",
  "node",
  "zip",
  "7z",
  "rar",
  "tar",
  "gz",
  "tgz",
  "bz2",
  "xz",
  "iso",
  "dmg",
  "pdf",
  "doc",
  "docx",
  "xls",
  "xlsx",
  "ppt",
  "pptx",
  "ttf",
  "otf",
  "woff",
  "woff2",
  "mp3",
  "wav",
  "flac",
  "ogg",
  "db",
  "sqlite"
]);

/** Whether `path`'s extension is one of `extensions`. */
function hasExtensionIn({ path, extensions }: {
  path: string;
  extensions: z.ZodType<string>;
}) {
  return extensions.safeParse(pathExtension(path)).success;
}

/** Whether `path`'s extension names a previewable image. */
export function isImagePath(path: string) {
  return hasExtensionIn({
    path,
    extensions: ImageExtension
  });
}

/** Whether `path`'s extension names a previewable video. */
export function isVideoPath(path: string) {
  return hasExtensionIn({
    path,
    extensions: VideoExtension
  });
}

/** Whether `path`'s extension names a markdown document. */
export function isMarkdownPath(path: string) {
  return hasExtensionIn({
    path,
    extensions: MarkdownExtension
  });
}

/** Whether `path`'s extension names a binary file a text editor can't open. */
export function isBinaryPath(path: string) {
  return hasExtensionIn({
    path,
    extensions: BinaryExtension
  });
}

/** Whether `path`'s extension names an HTML document. */
export function isHtmlPath(path: string) {
  return hasExtensionIn({
    path,
    extensions: HtmlExtension
  });
}

/** Whether `path` previews as a picture or a clip rather than as text. */
export function isMediaPath(path: string) {
  return isImagePath(path) || isVideoPath(path);
}

/** Whether `path` is a document with a rendered Preview beside its code - a
 *  markdown or HTML file. */
export function isRenderablePath(path: string) {
  return isMarkdownPath(path) || isHtmlPath(path);
}
