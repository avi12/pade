//! Video previews for the Change Feed: a short, small, web-playable clip of a
//! changed video file, transcoded by the user's own `ffmpeg`.
//!
//! The webview can only play a handful of containers/codecs, and a real video is
//! far too large to inline as a `data:` URL. Transcoding the opening seconds to a
//! scaled-down H.264/AAC MP4 solves both at once: any format ffmpeg reads (mov,
//! mkv, avi, …) previews, and the clip stays a bounded size. ffmpeg is an
//! optional external tool, never a bundled dependency — when it isn't installed
//! the card says so instead of previewing.

use std::io::Read;
use std::path::Path;
use std::process::Stdio;
use std::time::{Duration, Instant};

/// The video extensions the Change Feed previews (lower-case, no leading dot).
/// The one authoritative backend home for the set; its TS mirror is
/// `VideoExtension` in `@/lib/preview`, kept in sync by hand.
const VIDEO_EXTENSIONS: &[&str] = &[
    "mp4", "m4v", "mov", "webm", "mkv", "avi", "wmv", "flv", "mpg", "mpeg", "ogv", "3gp",
];

/// The MIME type of every clip [`preview_clip`] produces.
pub const PREVIEW_CLIP_MIME: &str = "video/mp4";

/// The executable transcoding the clip.
const TRANSCODER: &str = "ffmpeg";

/// How much of the source the clip covers — enough to see what the video is,
/// short enough to transcode in a moment.
const CLIP_SECONDS: &str = "15";

/// Scale down to at most 640px wide, keeping both dimensions even (H.264 with
/// 4:2:0 chroma rejects odd sizes) and the aspect ratio intact.
const SCALE_FILTER: &str = "scale=w='min(640,trunc(iw/2)*2)':h=-2";

/// Largest clip the feed will inline. Fifteen seconds at 640px lands well under
/// it; a pathological source is cut off rather than bloating the webview.
const MAX_CLIP_BYTES: u64 = 16 * 1024 * 1024;

/// A transcode that runs past this is killed — a preview is never worth a hang.
const TRANSCODE_TIMEOUT: Duration = Duration::from_secs(30);

/// How often the transcode's exit is polled while waiting on it.
const TRANSCODE_POLL: Duration = Duration::from_millis(50);

/// Why no clip was produced.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ClipError {
    /// `ffmpeg` isn't installed, so nothing can be transcoded.
    NoTranscoder,
    /// ffmpeg ran but produced no usable clip: an unreadable or corrupt source,
    /// a timeout, or output over [`MAX_CLIP_BYTES`].
    Failed,
}

impl std::fmt::Display for ClipError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NoTranscoder => formatter.write_str("ffmpeg is not installed"),
            Self::Failed => formatter.write_str("ffmpeg could not transcode the video"),
        }
    }
}

impl std::error::Error for ClipError {}

/// Whether `path`'s extension names a video the feed previews. Case-insensitive.
pub fn is_video(path: &Path) -> bool {
    path.extension()
        .and_then(|extension| extension.to_str())
        .is_some_and(|extension| {
            VIDEO_EXTENSIONS.contains(&extension.to_ascii_lowercase().as_str())
        })
}

/// The ffmpeg arguments turning `source` into a preview clip on stdout: its
/// first [`CLIP_SECONDS`], scaled by [`SCALE_FILTER`], as a fragmented MP4 (the
/// fragmented layout needs no seekable output, so it can stream to a pipe).
fn transcode_arguments(source: &Path) -> Vec<&std::ffi::OsStr> {
    let flags: [&str; 6] = [
        "-hide_banner",
        "-loglevel",
        "error",
        "-nostdin",
        "-t",
        CLIP_SECONDS,
    ];
    let encoding: [&str; 18] = [
        "-vf",
        SCALE_FILTER,
        "-c:v",
        "libx264",
        "-preset",
        "veryfast",
        "-crf",
        "28",
        "-pix_fmt",
        "yuv420p",
        "-c:a",
        "aac",
        "-b:a",
        "96k",
        "-movflags",
        "frag_keyframe+empty_moov",
        "-f",
        "mp4",
    ];
    flags
        .into_iter()
        .map(std::ffi::OsStr::new)
        .chain([std::ffi::OsStr::new("-i"), source.as_os_str()])
        .chain(encoding.into_iter().map(std::ffi::OsStr::new))
        .chain([std::ffi::OsStr::new("pipe:1")])
        .collect()
}

/// Transcode `source` into a preview clip ([`PREVIEW_CLIP_MIME`] bytes). Blocking
/// — call it off the async runtime. Kills ffmpeg on [`TRANSCODE_TIMEOUT`].
pub fn preview_clip(source: &Path) -> Result<Vec<u8>, ClipError> {
    let Some(transcoder) = crate::util::resolve(TRANSCODER) else {
        return Err(ClipError::NoTranscoder);
    };
    let mut child = crate::util::command(transcoder)
        .args(transcode_arguments(source))
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|_| ClipError::Failed)?;
    let Some(stdout) = child.stdout.take() else {
        let _ = child.kill();
        let _ = child.wait();
        return Err(ClipError::Failed);
    };

    // Drain stdout on its own thread: a child blocked on a full pipe never exits,
    // so waiting before reading would deadlock on any clip bigger than the pipe.
    let reader = std::thread::spawn(move || {
        let mut clip = Vec::new();
        let read = stdout
            .take(MAX_CLIP_BYTES.saturating_add(1))
            .read_to_end(&mut clip);
        read.map(|_| clip)
    });

    let deadline = Instant::now() + TRANSCODE_TIMEOUT;
    let succeeded = loop {
        match child.try_wait() {
            Ok(Some(status)) => break status.success(),
            Ok(None) if Instant::now() < deadline => std::thread::sleep(TRANSCODE_POLL),
            Ok(None) | Err(_) => {
                let _ = child.kill();
                let _ = child.wait();
                break false;
            }
        }
    };

    let clip = reader
        .join()
        .map_err(|_| ClipError::Failed)?
        .map_err(|_| ClipError::Failed)?;
    let is_within_cap = u64::try_from(clip.len()).is_ok_and(|length| length <= MAX_CLIP_BYTES);
    if !succeeded || clip.is_empty() || !is_within_cap {
        return Err(ClipError::Failed);
    }
    Ok(clip)
}

#[cfg(test)]
mod tests {
    use super::{is_video, preview_clip, transcode_arguments, ClipError};
    use std::path::Path;

    #[test]
    fn video_extensions_match_case_insensitively() {
        assert!(is_video(Path::new("clips/demo.mp4")));
        assert!(is_video(Path::new("Screen Recording.MOV")));
        assert!(is_video(Path::new("take.mkv")));
    }

    #[test]
    fn non_videos_are_not_videos() {
        assert!(!is_video(Path::new("main.rs")));
        assert!(!is_video(Path::new("README")));
        assert!(!is_video(Path::new("clip.mp4.zip")));
    }

    #[test]
    fn the_source_is_the_input_and_stdout_the_output() {
        let arguments = transcode_arguments(Path::new("in.mov"));
        let input = arguments
            .iter()
            .position(|argument| *argument == "-i")
            .expect("has an input flag");
        assert_eq!(arguments[input + 1], "in.mov");
        assert_eq!(
            arguments.last().copied(),
            Some(std::ffi::OsStr::new("pipe:1"))
        );
    }

    #[test]
    fn a_source_ffmpeg_cannot_read_fails_rather_than_yielding_an_empty_clip() {
        let scratch = std::env::temp_dir().join(format!("pade-media-{}.mp4", std::process::id()));
        std::fs::write(&scratch, b"not a video").expect("write scratch");
        let result = preview_clip(&scratch);
        let _ = std::fs::remove_file(&scratch);
        if result == Err(ClipError::NoTranscoder) {
            return;
        }
        assert_eq!(result, Err(ClipError::Failed));
    }

    #[test]
    fn a_real_video_transcodes_to_an_mp4_clip() {
        let Some(ffmpeg) = crate::util::resolve("ffmpeg") else {
            return;
        };
        let scratch =
            std::env::temp_dir().join(format!("pade-media-source-{}.mkv", std::process::id()));
        let generated = crate::util::command(ffmpeg)
            .args(["-hide_banner", "-loglevel", "error", "-y", "-f", "lavfi"])
            .args(["-i", "testsrc=duration=1:size=321x241:rate=10"])
            .arg(&scratch)
            .status()
            .expect("run ffmpeg");
        assert!(generated.success());

        let clip = preview_clip(&scratch);
        let _ = std::fs::remove_file(&scratch);
        let clip = clip.expect("transcode the generated video");
        let file_type_box = clip.get(4..8);
        assert_eq!(file_type_box, Some(b"ftyp".as_slice()));
    }
}
