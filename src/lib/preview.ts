// Which Change Feed changes are previewable, and as what, decided by file
// extension. The single authoritative TS home for every previewable-file
// extension set — images (rendered inline via `<img src>`), markdown (rendered
// to HTML), and HTML (rendered as-is) — so the classifiers can never disagree
// about which renderer a card routes to. The image list mirrors the backend's
// IMAGE_MIME_TYPES (watcher.rs), where each extension is also mapped to its MIME
// type; the two lists are kept in sync by hand.

import { z } from "zod";

/** The image file extensions the Change Feed previews inline (lower-case, no
 *  leading dot). SVG is included — it is rendered through `<img src>` like every
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

/** The markdown extensions the Change Feed can render to a preview. */
const MarkdownExtension = z.enum(["md", "markdown"]);

/** The HTML extensions the Change Feed can render (inertly) as a preview. */
const HtmlExtension = z.enum(["html", "htm"]);

const IMAGE_EXTENSIONS: readonly string[] = ImageExtension.options;
const MARKDOWN_EXTENSIONS: readonly string[] = MarkdownExtension.options;
const HTML_EXTENSIONS: readonly string[] = HtmlExtension.options;

/** `path`'s lower-cased final extension, or `null` when the base name carries no
 *  extension (a dotfile or an extensionless path). Case-insensitive; the one
 *  shared classifier all three predicates below decide from. */
function extensionOf(path: string): string | null {
  const base = path.split(/[\\/]/).pop() ?? path;
  const dot = base.lastIndexOf(".");
  const hasExtension = dot > 0 && dot < base.length - 1;
  if (!hasExtension) {
    return null;
  }

  return base.slice(dot + 1).toLowerCase();
}

/** Whether `path`'s extension names a previewable image. */
export function isImagePath(path: string): boolean {
  const extension = extensionOf(path);
  return extension !== null && IMAGE_EXTENSIONS.includes(extension);
}

/** Whether `path`'s extension names a markdown document. */
export function isMarkdownPath(path: string): boolean {
  const extension = extensionOf(path);
  return extension !== null && MARKDOWN_EXTENSIONS.includes(extension);
}

/** Whether `path`'s extension names an HTML document. */
export function isHtmlPath(path: string): boolean {
  const extension = extensionOf(path);
  return extension !== null && HTML_EXTENSIONS.includes(extension);
}
