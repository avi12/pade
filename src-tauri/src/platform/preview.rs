//! The rich file previews the Change Feed and the commit view share: an image
//! as a `data:` URL, a video as a transcoded clip, a document as its UTF-8
//! text. Each caller decides which bytes it may read (the feed's watch-root
//! confinement, a commit's git object); this module only turns those bytes into
//! the one wire shape both views render.

use std::path::{Path, PathBuf};

use serde::Serialize;

use crate::platform::media::{preview_clip, ClipError, PREVIEW_CLIP_MIME};

/// Largest file the Change Feed will snapshot for a baseline or read for a live
/// preview, and the largest committed document the commit view renders. A few
/// hundred KB covers real source files; past it the preview falls back to "No
/// preview available" rather than holding megabytes per touched path.
pub(crate) const MAXIMUM_PREVIEW_BYTES: u64 = 512 * 1024;

/// Largest image a preview will inline as a data URL. A few megabytes covers
/// real project assets; past it the preview is skipped rather than bloating the
/// webview with megabytes of base64 for one picture.
pub(crate) const MAXIMUM_IMAGE_BYTES: u64 = 5 * 1024 * 1024;

/// The image extensions previewed inline, each paired with the MIME type its
/// `data:` URL carries. The one authoritative home for the extension→MIME
/// mapping (its TS mirror is `ImageExtension` in `@/lib/feed/preview/preview`);
/// an extension absent here is not treated as a previewable image. SVG is
/// included and served as a data URL like the raster formats - the frontend
/// renders every image through `<img src>`, so untrusted SVG markup is never
/// inlined into the DOM (no scripts run, no external refs load).
const IMAGE_MIME_TYPES: &[(&str, &str)] = &[
    ("png", "image/png"),
    ("jpg", "image/jpeg"),
    ("jpeg", "image/jpeg"),
    ("gif", "image/gif"),
    ("webp", "image/webp"),
    ("avif", "image/avif"),
    ("bmp", "image/bmp"),
    ("ico", "image/x-icon"),
    ("svg", "image/svg+xml"),
];

/// The MIME type for `path`'s extension when it names a previewable image, else
/// `None`. Case-insensitive on the extension.
pub(crate) fn image_mime_type(path: &Path) -> Option<&'static str> {
    let extension = path.extension().and_then(|name| name.to_str())?;
    IMAGE_MIME_TYPES
        .iter()
        .find(|(name, _)| name.eq_ignore_ascii_case(extension))
        .map(|(_, mime)| *mime)
}

/// Standard-alphabet (RFC 4648) base64 for the data URLs - a few lines of std
/// rather than a new dependency (the only base64 the backend needs). Encodes
/// each three input bytes into four characters, padding a final partial group
/// with `=`.
fn base64_encode(bytes: &[u8]) -> String {
    const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    const PAD: char = '=';
    let sextet = |value: u8| char::from(ALPHABET[usize::from(value & 0x3f)]);

    let mut encoded = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let first = chunk[0];
        let second = chunk.get(1).copied().unwrap_or(0);
        let third = chunk.get(2).copied().unwrap_or(0);
        encoded.push(sextet(first >> 2));
        encoded.push(sextet((first << 4) | (second >> 4)));
        encoded.push(if chunk.len() > 1 {
            sextet((second << 2) | (third >> 6))
        } else {
            PAD
        });
        encoded.push(if chunk.len() > 2 { sextet(third) } else { PAD });
    }
    encoded
}

/// A `data:<mime>;base64,<bytes>` URL - the one shape every inline media preview
/// hands the frontend, so it renders with a plain `src` and no asset protocol.
fn data_url(mime: &str, bytes: &[u8]) -> String {
    format!("data:{mime};base64,{}", base64_encode(bytes))
}

/// An inline image preview: the image's bytes as a ready-to-use `data:` URL, so
/// the frontend renders it with a plain `<img src>` and needs no asset protocol
/// or extra capability.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FeedImage {
    /// A `data:<mime>;base64,<bytes>` URL - directly usable as an `<img>` source.
    data_url: String,
}

impl FeedImage {
    /// The preview of `bytes` read from `path`, tagged with the MIME type of its
    /// extension. `None` when `path` does not name a previewable image.
    pub(crate) fn of(path: &Path, bytes: &[u8]) -> Option<Self> {
        let mime = image_mime_type(path)?;
        Some(Self {
            data_url: data_url(mime, bytes),
        })
    }
}

/// A video preview, tagged by `kind` so the view can tell "here is a clip" from
/// "install ffmpeg to see one".
#[derive(Debug, Serialize)]
#[serde(
    tag = "kind",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum FeedVideo {
    /// A short transcoded clip as a `data:` URL - directly usable as a `<video>` source.
    Ready { data_url: String },
    /// `ffmpeg` isn't installed, so no clip can be made.
    NoTranscoder,
}

/// The video preview of the file at `source`: its opening seconds transcoded by
/// ffmpeg (see [`crate::platform::media`]) on a blocking thread, so a slow video
/// never stalls the async runtime. `None` when ffmpeg couldn't read it.
pub(crate) async fn video_preview(source: PathBuf) -> Result<Option<FeedVideo>, String> {
    let clip = tauri::async_runtime::spawn_blocking(move || preview_clip(&source))
        .await
        .map_err(|error| error.to_string())?;
    match clip {
        Ok(bytes) => Ok(Some(FeedVideo::Ready {
            data_url: data_url(PREVIEW_CLIP_MIME, &bytes),
        })),
        Err(ClipError::NoTranscoder) => Ok(Some(FeedVideo::NoTranscoder)),
        Err(ClipError::Failed) => Ok(None),
    }
}

/// `bytes` as preview text: valid UTF-8 with no NUL, else `None` (binary).
pub(crate) fn preview_text(bytes: Vec<u8>) -> Option<String> {
    if bytes.contains(&0) {
        return None;
    }
    String::from_utf8(bytes).ok()
}

#[cfg(test)]
mod tests {
    use super::{base64_encode, image_mime_type, preview_text, FeedImage};
    use std::path::Path;

    #[test]
    fn image_mime_type_maps_known_extensions_case_insensitively() {
        assert_eq!(
            image_mime_type(Path::new("a/b/logo.png")),
            Some("image/png")
        );
        assert_eq!(image_mime_type(Path::new("photo.JPG")), Some("image/jpeg"));
        assert_eq!(image_mime_type(Path::new("photo.jpeg")), Some("image/jpeg"));
        assert_eq!(
            image_mime_type(Path::new("icon.svg")),
            Some("image/svg+xml")
        );
        assert_eq!(image_mime_type(Path::new("art.avif")), Some("image/avif"));
        assert_eq!(
            image_mime_type(Path::new("favicon.ico")),
            Some("image/x-icon")
        );
    }

    #[test]
    fn image_mime_type_is_none_for_non_images() {
        assert_eq!(image_mime_type(Path::new("main.rs")), None);
        assert_eq!(image_mime_type(Path::new("README")), None);
        // Only the final extension decides; a `.png` in the stem doesn't count.
        assert_eq!(image_mime_type(Path::new("archive.png.zip")), None);
    }

    /// The RFC 4648 test vectors - proof the hand-rolled encoder pads partial
    /// groups correctly (the case a data URL depends on).
    #[test]
    fn base64_encode_matches_the_rfc_vectors() {
        assert_eq!(base64_encode(b""), "");
        assert_eq!(base64_encode(b"f"), "Zg==");
        assert_eq!(base64_encode(b"fo"), "Zm8=");
        assert_eq!(base64_encode(b"foo"), "Zm9v");
        assert_eq!(base64_encode(b"foob"), "Zm9vYg==");
        assert_eq!(base64_encode(b"fooba"), "Zm9vYmE=");
        assert_eq!(base64_encode(b"foobar"), "Zm9vYmFy");
    }

    #[test]
    fn an_image_preview_is_a_data_url_of_its_mime_type() {
        let preview = FeedImage::of(Path::new("logo.png"), b"foo").expect("a png previews");
        assert_eq!(preview.data_url, "data:image/png;base64,Zm9v");
        assert!(FeedImage::of(Path::new("notes.txt"), b"foo").is_none());
    }

    #[test]
    fn preview_text_rejects_binary_content() {
        assert_eq!(
            preview_text(b"# Title".to_vec()).as_deref(),
            Some("# Title")
        );
        assert_eq!(preview_text(b"a\0b".to_vec()), None);
        assert_eq!(preview_text(vec![0xff, 0xfe]), None);
    }
}
