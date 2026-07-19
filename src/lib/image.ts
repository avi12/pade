// Whether a Change Feed change is a previewable image, decided by its file
// extension. The single authoritative TS home for the previewable-image
// extension set — it mirrors the backend's IMAGE_MIME_TYPES (watcher.rs), where
// each extension is also mapped to its MIME type; the two lists are kept in sync
// by hand (the backend serves the bytes, the frontend only decides which cards
// to route to the image renderer).

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

const IMAGE_EXTENSIONS: readonly string[] = ImageExtension.options;

/** Whether `path`'s extension names a previewable image. Case-insensitive; a
 *  dotfile or extensionless path is never an image. */
export function isImagePath(path: string): boolean {
  const base = path.split(/[\\/]/).pop() ?? path;
  const dot = base.lastIndexOf(".");
  const hasExtension = dot > 0 && dot < base.length - 1;
  if (!hasExtension) {
    return false;
  }

  const extension = base.slice(dot + 1).toLowerCase();
  return IMAGE_EXTENSIONS.includes(extension);
}
