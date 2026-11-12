//! Finding the chats on disk: every agent's recorded chats with the size of all
//! each leaves behind - Claude's transcript, per-chat folders, Codex's rollout,
//! Copilot's session folder - read into `Chat` rows for the limits to judge.

use std::collections::HashMap;
use std::ffi::OsString;
use std::fs::DirEntry;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

use super::{Chat, CLAUDE_PER_CHAT_FOLDERS, COPILOT_EVENT_LOG, UNKNOWN_PROJECT};
use crate::agents::conversations::{
    copilot_workspace_cwd, COPILOT_WORKSPACE_FILE, TRANSCRIPT_EXTENSION,
};
use crate::agents::resume::{first_line, recorded_cwd, rollout_files, CODEX_SESSIONS_DIRECTORY};
use crate::agents::running_model::copilot_session_root;
use crate::agents::{
    claude_directory, claude_projects_directory, codex_home, ID_CLAUDE, ID_CODEX, ID_COPILOT,
};
use crate::util::{encode_project, milliseconds_since_unix_epoch};

/// Every chat of every agent PADE knows the files of.
pub(super) fn recorded_chats() -> Vec<Chat> {
    let mut chats = claude_chats();
    chats.extend(codex_chats());
    chats.extend(copilot_chats());
    chats
}

/// The one project key for a working directory, whichever agent recorded it.
fn project_key(cwd: &str) -> String {
    encode_project(cwd.trim_end_matches(['\\', '/'])).to_lowercase()
}

fn modified_at(path: &Path) -> Option<u64> {
    let modified: SystemTime = std::fs::metadata(path).ok()?.modified().ok()?;
    Some(milliseconds_since_unix_epoch(modified))
}

/// Whether a listed entry is a real folder - a link to one isn't followed, so
/// nothing outside is counted or deleted.
fn is_folder(entry: &DirEntry) -> bool {
    entry
        .file_type()
        .is_ok_and(|file_type| file_type.is_dir() && !file_type.is_symlink())
}

/// The bytes of everything under `folder`. Sizes come from the listing itself,
/// which on Windows costs no extra file open - tens of thousands of chats
/// measure in about a second instead of half a minute.
fn folder_size(folder: &Path) -> u64 {
    let Ok(entries) = std::fs::read_dir(folder) else {
        return 0;
    };
    entries
        .flatten()
        .map(|entry| {
            if is_folder(&entry) {
                return folder_size(&entry.path());
            }

            entry.metadata().map_or(0, |metadata| metadata.len())
        })
        .sum()
}

/// Each sub-folder of `parent` by name, with its size.
fn sub_folder_sizes(parent: &Path) -> HashMap<OsString, (PathBuf, u64)> {
    let Ok(entries) = std::fs::read_dir(parent) else {
        return HashMap::new();
    };
    entries
        .flatten()
        .filter(is_folder)
        .map(|entry| {
            let path = entry.path();
            let size = folder_size(&path);
            (entry.file_name(), (path, size))
        })
        .collect()
}

fn claude_chats() -> Vec<Chat> {
    let (Some(projects), Some(claude)) = (claude_projects_directory(), claude_directory()) else {
        return Vec::new();
    };
    let Ok(project_folders) = std::fs::read_dir(projects) else {
        return Vec::new();
    };
    let per_chat_folders: Vec<_> = CLAUDE_PER_CHAT_FOLDERS
        .iter()
        .map(|name| sub_folder_sizes(&claude.join(name)))
        .collect();

    let mut chats = Vec::new();
    for folder in project_folders.flatten().filter(is_folder) {
        let project = folder.file_name().to_string_lossy().to_lowercase();
        let sibling_folders = sub_folder_sizes(&folder.path());
        let Ok(entries) = std::fs::read_dir(folder.path()) else {
            continue;
        };
        for entry in entries.flatten() {
            let transcript = entry.path();
            let is_transcript = transcript
                .extension()
                .is_some_and(|extension| extension == TRANSCRIPT_EXTENSION);
            let (true, Ok(metadata), Some(id)) =
                (is_transcript, entry.metadata(), transcript.file_stem())
            else {
                continue;
            };
            let Ok(modified) = metadata.modified() else {
                continue;
            };

            let mut paths = vec![transcript.clone()];
            let mut bytes = metadata.len();
            let owned = std::iter::once(&sibling_folders)
                .chain(&per_chat_folders)
                .filter_map(|folders| folders.get(id));
            for (path, size) in owned {
                paths.push(path.clone());
                bytes += size;
            }
            chats.push(Chat {
                agent_id: ID_CLAUDE,
                project: project.clone(),
                updated_at: milliseconds_since_unix_epoch(modified),
                bytes,
                paths,
            });
        }
    }
    chats
}

fn codex_chats() -> Vec<Chat> {
    let Some(home) = codex_home() else {
        return Vec::new();
    };
    rollout_files(&home.join(CODEX_SESSIONS_DIRECTORY))
        .into_iter()
        .map(|(modified, path)| {
            let project = first_line(&path)
                .and_then(|line| recorded_cwd(&line))
                .map_or_else(|| UNKNOWN_PROJECT.to_string(), |cwd| project_key(&cwd));
            Chat {
                agent_id: ID_CODEX,
                project,
                updated_at: milliseconds_since_unix_epoch(modified),
                bytes: std::fs::metadata(&path).map_or(0, |metadata| metadata.len()),
                paths: vec![path],
            }
        })
        .collect()
}

fn copilot_chats() -> Vec<Chat> {
    let Some(root) = copilot_session_root() else {
        return Vec::new();
    };
    sub_folder_sizes(&root)
        .into_values()
        .filter_map(|(folder, bytes)| {
            let updated_at =
                modified_at(&folder.join(COPILOT_EVENT_LOG)).or_else(|| modified_at(&folder))?;
            let project = copilot_workspace_cwd(&folder.join(COPILOT_WORKSPACE_FILE))
                .map_or_else(|| UNKNOWN_PROJECT.to_string(), |cwd| project_key(&cwd));
            Some(Chat {
                agent_id: ID_COPILOT,
                project,
                updated_at,
                bytes,
                paths: vec![folder],
            })
        })
        .collect()
}
