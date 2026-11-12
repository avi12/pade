//! Where PADE was launched: the launch directory, its canonical form and the launch context the frontend boots from.

use super::store::is_project;
use super::store::LaunchContext;
use std::path::Path;
use std::path::PathBuf;

/// The directory PADE launched into, and whether it came from an explicit request.
pub(crate) struct LaunchDirectory {
    pub path: PathBuf,
    /// `true` when a `pade <dir>` argument named it (an explicit open), so the
    /// caller treats it as a project without probing for markers.
    pub explicit: bool,
}

/// Resolve the directory this instance launched into - the single source of truth
/// for both `launch_context` (what the frontend boots into) and the per-instance
/// `WebView2` folder keying (which project's process tree this instance owns). A
/// directory passed as an argument - `pade <dir>` from a terminal or the folder's
/// context menu - is an explicit request to open that project; otherwise it is the
/// process working directory.
pub(crate) fn launch_directory() -> LaunchDirectory {
    let directory_argument = std::env::args()
        .skip(1)
        .find(|argument| Path::new(argument).is_dir());
    if let Some(directory) = directory_argument {
        return LaunchDirectory {
            path: PathBuf::from(directory),
            explicit: true,
        };
    }
    LaunchDirectory {
        path: std::env::current_dir().unwrap_or_default(),
        explicit: false,
    }
}

/// The launch directory, canonicalized when it can be - the identity of this
/// app instance. Two instances launched into different projects are different
/// instances (their own `WebView2` folder, their own PTY host); a relaunch into
/// the same one is the same instance again.
pub(crate) fn canonical_launch_directory() -> PathBuf {
    let launch = launch_directory().path;
    std::fs::canonicalize(&launch).unwrap_or(launch)
}

/// This instance's `WebView2` user-data folder, keyed by the launch project so two
/// projects open in parallel each run in their own browser + GPU process tree
/// instead of sharing one - the shared default lets one instance's GPU load (and
/// the ~16 `WebGL`-context cap) compound into the other and trip a DWM/GPU reset.
/// Reopening the same project reuses its folder (a stable digest of the canonical
/// path), so nothing accumulates per launch. `None` when `LOCALAPPDATA` can't be
/// resolved, leaving `WebView2` on its shared default. Windows-only - the folder is
/// a `WebView2` concept.
#[cfg(windows)]
pub(crate) fn webview_data_directory() -> Option<PathBuf> {
    use std::borrow::Cow;

    const WEBVIEW_DATA_DIRECTORY: &str = "webview2";
    /// The folder-name stem for a launch directory with no name (a drive root).
    const UNNAMED_PROJECT: &str = "project";

    let base = crate::util::pade_data_directory()?;
    let canonical = canonical_launch_directory();
    let key = canonical.to_string_lossy().to_lowercase();

    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    std::hash::Hash::hash(&key, &mut hasher);
    let digest = std::hash::Hasher::finish(&hasher);

    let name = canonical
        .file_name()
        .map_or(Cow::Borrowed(UNNAMED_PROJECT), |name| {
            name.to_string_lossy()
        });
    Some(
        base.join(WEBVIEW_DATA_DIRECTORY)
            .join(format!("{name}-{digest:016x}")),
    )
}

#[tauri::command]
pub fn launch_context() -> LaunchContext {
    let launch = launch_directory();
    LaunchContext {
        has_project: launch.explicit || is_project(&launch.path),
        cwd: launch.path.to_string_lossy().into_owned(),
    }
}
