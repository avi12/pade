//! Where a PTY host lives on disk and how the app finds it: the per-instance
//! folder with its state file (`host.json` - endpoint, token, process id), and
//! the folder of build-named executable copies the hosts run from.
//!
//! A host runs from a *copy* of the app's executable, never the executable
//! itself: Windows locks a running exe, so a host running `pade.exe` would make
//! every later `cargo build` fail to link it, and the updater fail to replace it.

use std::io;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

/// PADE's data folder holds every host under this name.
const HOST_DIRECTORY: &str = "pty-host";
/// Where the executable copies live, beside the instance folders.
const BINARIES_DIRECTORY: &str = "bin";
const STATE_FILE: &str = "host.json";
const LOG_FILE: &str = "host.log";
/// A copy's file name: this prefix, the build fingerprint, the platform suffix.
const COPY_PREFIX: &str = "pade-pty-host-";
/// How many hex digits of a digest name a build or an instance.
const DIGEST_HEX_DIGITS: usize = 16;
/// The folder-name stem for a launch directory with no name (a drive root).
const UNNAMED_INSTANCE: &str = "project";

/// The folders one app instance's host uses.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct HostPaths {
    instance_directory: PathBuf,
    binaries_directory: PathBuf,
}

impl HostPaths {
    /// The host folders for instance `key` under PADE's data folder.
    pub(crate) fn for_instance(key: &str) -> Option<Self> {
        let root = crate::util::pade_data_directory()?.join(HOST_DIRECTORY);
        Some(Self::under(&root, key))
    }

    /// The host folders for instance `key` under `root`.
    pub(crate) fn under(root: &Path, key: &str) -> Self {
        Self {
            instance_directory: root.join(key),
            binaries_directory: root.join(BINARIES_DIRECTORY),
        }
    }

    /// The host folders around an instance directory a host was started with.
    pub(crate) fn from_instance_directory(instance_directory: PathBuf) -> Self {
        let binaries_directory = instance_directory.parent().map_or_else(
            || PathBuf::from(BINARIES_DIRECTORY),
            |root| root.join(BINARIES_DIRECTORY),
        );
        Self {
            instance_directory,
            binaries_directory,
        }
    }

    pub(crate) fn instance_directory(&self) -> &Path {
        &self.instance_directory
    }

    pub(crate) fn state_file(&self) -> PathBuf {
        self.instance_directory.join(STATE_FILE)
    }

    pub(crate) fn log_file(&self) -> PathBuf {
        self.instance_directory.join(LOG_FILE)
    }
}

/// The instance key for an app launched into `launch_directory`: its name (for
/// a human reading the folder) plus a stable digest of its lowercased path. A
/// sha256 digest rather than std's hasher, whose output may change between Rust
/// releases - a key that moved would orphan a running host's agents.
pub(crate) fn instance_key(launch_directory: &Path) -> String {
    // Lowercased whole, name included: Windows paths ignore case, so two
    // spellings of one folder are one instance.
    let identity = PathBuf::from(launch_directory.to_string_lossy().to_lowercase());
    let name = identity.file_name().map_or_else(
        || UNNAMED_INSTANCE.to_string(),
        |name| name.to_string_lossy().into_owned(),
    );
    format!(
        "{name}-{}",
        short_digest(identity.as_os_str().as_encoded_bytes())
    )
}

/// What the state file records about a running host, written by the host once
/// it listens. The token lives only here, in the user's own data folder.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub(crate) struct HostState {
    pub(crate) protocol_version: u32,
    pub(crate) endpoint: String,
    pub(crate) token: String,
    pub(crate) process_id: u32,
    pub(crate) executable_fingerprint: String,
}

/// The state file's contents, `None` when it is missing or unreadable - both
/// mean there is no host to reach.
pub(crate) fn read_state(path: &Path) -> Option<HostState> {
    let text = std::fs::read_to_string(path).ok()?;
    serde_json::from_str(&text).ok()
}

/// Write the state file atomically (a temporary file renamed over it), so a
/// reader never sees half of one.
pub(crate) fn write_state(path: &Path, state: &HostState) -> io::Result<()> {
    let text = serde_json::to_string(state).map_err(io::Error::other)?;
    let temporary = path.with_extension(format!("{}.tmp", state.process_id));
    std::fs::write(&temporary, text)?;
    std::fs::rename(&temporary, path)
}

/// Remove the state file only when it still describes host `process_id` - a
/// successor may already have replaced it.
pub(crate) fn remove_state_if_owned(path: &Path, process_id: u32) {
    let is_ours = read_state(path).is_some_and(|state| state.process_id == process_id);
    if !is_ours {
        return;
    }
    if let Err(error) = std::fs::remove_file(path) {
        eprintln!("[pty-host] could not remove {}: {error}", path.display());
    }
}

/// A fingerprint of the build `executable` holds: its size and last-write time.
/// Not a content hash - hashing a debug build (hundreds of megabytes) on every
/// app start would delay the first terminal by seconds, and every rebuild
/// changes the write time anyway.
pub(crate) fn executable_fingerprint(executable: &Path) -> io::Result<String> {
    let metadata = std::fs::metadata(executable)?;
    let written = metadata
        .modified()
        .map(crate::util::milliseconds_since_unix_epoch)
        .unwrap_or_default();
    let identity = format!("{}:{written}", metadata.len());
    Ok(short_digest(identity.as_bytes()))
}

/// The fingerprint a host copy's file name carries, `None` for any other file.
pub(crate) fn fingerprint_of_copy(executable: &Path) -> Option<String> {
    let stem = executable.file_stem()?.to_str()?;
    stem.strip_prefix(COPY_PREFIX).map(ToString::to_string)
}

/// The copy of `executable` a host for build `fingerprint` runs from, made when
/// it is missing. Copied under a temporary name and renamed into place, so a
/// half-written copy is never launched.
pub(crate) fn ensure_host_copy(
    paths: &HostPaths,
    executable: &Path,
    fingerprint: &str,
) -> io::Result<PathBuf> {
    let file_name = format!("{COPY_PREFIX}{fingerprint}{}", std::env::consts::EXE_SUFFIX);
    let copy = paths.binaries_directory.join(file_name);
    if copy.is_file() {
        return Ok(copy);
    }
    std::fs::create_dir_all(&paths.binaries_directory)?;
    let temporary = copy.with_extension(format!("{}.tmp", uuid::Uuid::new_v4().simple()));
    std::fs::copy(executable, &temporary)?;
    if let Err(error) = std::fs::rename(&temporary, &copy) {
        // Another app instance placed the same copy first; theirs is identical.
        std::fs::remove_file(&temporary)?;
        if !copy.is_file() {
            return Err(error);
        }
    }
    Ok(copy)
}

/// Delete host copies other than the builds in `keep`. A copy a running host
/// still executes is locked by Windows, so its deletion fails and it stays -
/// exactly the copies that must.
pub(crate) fn prune_host_copies(paths: &HostPaths, keep: &[&str]) {
    let Ok(entries) = std::fs::read_dir(&paths.binaries_directory) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        let Some(fingerprint) = fingerprint_of_copy(&path) else {
            continue;
        };
        if keep.contains(&fingerprint.as_str()) {
            continue;
        }
        if let Err(error) = std::fs::remove_file(&path) {
            let is_in_use = error.kind() == io::ErrorKind::PermissionDenied;
            if !is_in_use {
                eprintln!("[pty-host] could not prune {}: {error}", path.display());
            }
        }
    }
}

fn short_digest(bytes: &[u8]) -> String {
    let digest = crate::util::lowercase_hex(&Sha256::digest(bytes));
    digest[..DIGEST_HEX_DIGITS].to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch(name: &str) -> PathBuf {
        let directory =
            std::env::temp_dir().join(format!("pade-pty-host-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&directory);
        std::fs::create_dir_all(&directory).expect("scratch directory");
        directory
    }

    fn state(process_id: u32) -> HostState {
        HostState {
            protocol_version: 1,
            endpoint: r"\\.\pipe\x".into(),
            token: "t".into(),
            process_id,
            executable_fingerprint: "f".into(),
        }
    }

    #[test]
    fn instance_keys_are_stable_and_case_blind() {
        let key = instance_key(Path::new(r"C:\Work\Pade"));
        assert_eq!(key, instance_key(Path::new(r"c:\work\pade")));
        assert!(key.starts_with("pade-"));
        assert_ne!(key, instance_key(Path::new(r"C:\Work\Other")));
    }

    #[test]
    fn state_round_trips_and_garbage_reads_as_absent() {
        let directory = scratch("state");
        let path = directory.join(STATE_FILE);
        assert_eq!(read_state(&path), None);
        write_state(&path, &state(7)).expect("written");
        assert_eq!(read_state(&path), Some(state(7)));
        std::fs::write(&path, "not json").expect("written");
        assert_eq!(read_state(&path), None);
        let _ = std::fs::remove_dir_all(&directory);
    }

    #[test]
    fn a_host_never_removes_its_successors_state() {
        let directory = scratch("successor");
        let path = directory.join(STATE_FILE);
        write_state(&path, &state(8)).expect("written");
        remove_state_if_owned(&path, 7);
        assert_eq!(
            read_state(&path),
            Some(state(8)),
            "the successor's file stays"
        );
        remove_state_if_owned(&path, 8);
        assert_eq!(read_state(&path), None);
        let _ = std::fs::remove_dir_all(&directory);
    }

    #[test]
    fn a_copy_is_made_once_and_named_by_its_build() {
        let directory = scratch("copy");
        let paths = HostPaths::under(&directory, "instance");
        let executable = directory.join("source.bin");
        std::fs::write(&executable, b"build").expect("written");
        let fingerprint = executable_fingerprint(&executable).expect("fingerprint");
        let copy = ensure_host_copy(&paths, &executable, &fingerprint).expect("copied");
        assert_eq!(fingerprint_of_copy(&copy), Some(fingerprint.clone()));
        assert_eq!(
            ensure_host_copy(&paths, &executable, &fingerprint).expect("reused"),
            copy
        );

        let stale = ensure_host_copy(&paths, &executable, "stale").expect("copied");
        prune_host_copies(&paths, &[fingerprint.as_str()]);
        assert!(copy.is_file());
        assert!(!stale.exists());
        let _ = std::fs::remove_dir_all(&directory);
    }

    #[test]
    fn a_hosts_paths_derive_from_its_instance_directory() {
        let root = std::env::temp_dir().join("hosts");
        let paths = HostPaths::under(&root, "key");
        assert_eq!(
            HostPaths::from_instance_directory(paths.instance_directory().to_path_buf()),
            paths
        );
    }
}
