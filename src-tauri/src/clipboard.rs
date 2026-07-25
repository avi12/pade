//! Clipboard image capture — lets a terminal Ctrl+V paste an image into an
//! agent session. TUIs cannot read the OS clipboard's bitmap themselves, so the
//! standard pattern is: save the bitmap to a temp file and paste its *path*;
//! agent composers (Claude Code, `OpenCode`) then attach the file.

use std::fs;
use std::io::BufWriter;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use tauri_plugin_clipboard_manager::ClipboardExt;

/// Saved pastes older than this are pruned on the next paste, so the folder
/// stays bounded while a path pasted into a long-running session keeps working.
const RETAIN_FOR: Duration = Duration::from_secs(3 * 24 * 60 * 60);

/// The stable folder every clipboard-image paste lands in.
fn paste_dir() -> PathBuf {
    std::env::temp_dir().join("pade-paste")
}

/// Which of `entries` (path + last-modified) are old enough to delete.
fn stale_paths(entries: &[(PathBuf, SystemTime)], now: SystemTime) -> Vec<PathBuf> {
    entries
        .iter()
        .filter(|(_, modified)| {
            now.duration_since(*modified)
                .is_ok_and(|age| age > RETAIN_FOR)
        })
        .map(|(path, _)| path.clone())
        .collect()
}

/// Best-effort: a prune failure must never block the paste itself.
fn prune_old_pastes(directory: &Path) {
    let Ok(read) = fs::read_dir(directory) else {
        return;
    };
    let entries: Vec<(PathBuf, SystemTime)> = read
        .flatten()
        .filter_map(|entry| {
            let modified = entry.metadata().ok()?.modified().ok()?;
            Some((entry.path(), modified))
        })
        .collect();
    for path in stale_paths(&entries, SystemTime::now()) {
        let _ = fs::remove_file(path);
    }
}

fn encode_png(image: &tauri::image::Image<'_>, path: &Path) -> Result<(), String> {
    let file = fs::File::create(path).map_err(|e| e.to_string())?;
    let mut encoder = png::Encoder::new(BufWriter::new(file), image.width(), image.height());
    encoder.set_color(png::ColorType::Rgba);
    encoder.set_depth(png::BitDepth::Eight);
    let mut writer = encoder.write_header().map_err(|e| e.to_string())?;
    writer
        .write_image_data(image.rgba())
        .map_err(|e| e.to_string())?;
    writer.finish().map_err(|e| e.to_string())
}

/// If the clipboard currently holds a bitmap, save it as a PNG under a stable
/// temp folder and return the absolute path; `None` when there is no image.
/// Async so the clipboard read + file write never block the main thread.
#[tauri::command]
pub async fn clipboard_image_save(app: tauri::AppHandle) -> Result<Option<String>, String> {
    // The plugin reports "no image available" as an error variant, not a
    // distinct case — so any read failure means "fall back to text paste".
    let Ok(image) = app.clipboard().read_image() else {
        return Ok(None);
    };

    let directory = paste_dir();
    fs::create_dir_all(&directory).map_err(|e| e.to_string())?;
    prune_old_pastes(&directory);

    let millis = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|e| e.to_string())?
        .as_millis();
    let path = directory.join(format!("paste-{millis}.png"));
    encode_png(&image, &path)?;
    Ok(Some(path.to_string_lossy().into_owned()))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn seconds(count: u64) -> Duration {
        Duration::from_secs(count)
    }

    #[test]
    fn a_fresh_paste_is_kept() {
        let now = SystemTime::now();
        let entries = vec![(PathBuf::from("paste-1.png"), now - seconds(60))];
        assert!(stale_paths(&entries, now).is_empty());
    }

    #[test]
    fn a_paste_past_the_retention_window_is_stale() {
        let now = SystemTime::now();
        let old = now - (RETAIN_FOR + seconds(1));
        let recent = now - seconds(60);
        let entries = vec![
            (PathBuf::from("paste-old.png"), old),
            (PathBuf::from("paste-new.png"), recent),
        ];
        assert_eq!(
            stale_paths(&entries, now),
            vec![PathBuf::from("paste-old.png")]
        );
    }

    #[test]
    fn a_future_timestamp_is_never_stale() {
        let now = SystemTime::now();
        let entries = vec![(PathBuf::from("paste-future.png"), now + seconds(60))];
        assert!(stale_paths(&entries, now).is_empty());
    }
}
