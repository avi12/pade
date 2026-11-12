//! The disk the agents' recorded chats take, and keeping it under the user's
//! limits - the same idea as the Claude app's `/storage` panel, but with caps
//! the user sets instead of a fixed 30-day cutoff.
//!
//! A chat is everything one conversation leaves behind:
//!
//!   * Claude Code - `~/.claude/projects/<project>/<id>.jsonl`, its sibling
//!     `<id>/` folder (subagent transcripts, saved tool output), and the
//!     `file-history/<id>` and `session-env/<id>` folders under `~/.claude`;
//!   * Codex - one rollout under `sessions/<yyyy>/<mm>/<dd>/`;
//!   * Copilot - one `session-state/<id>/` folder.
//!
//! Two limits, both off until the user sets them: one chat's size, and all
//! chats together. A limit never deletes by itself: it picks the chats over
//! it - the oldest chat of each project in turn, so one old project is never
//! wiped out before a busy one gives anything up - and the user ticks which
//! of those go. A project's newest chat and every chat used in the last week
//! are never picked, which also keeps a running agent's conversation safe.

use std::collections::{BTreeMap, HashMap, HashSet};
use std::ffi::OsString;
use std::fs::DirEntry;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

use serde::Serialize;

use crate::agents::conversations::{
    claude_transcript_title, codex_titles, copilot_workspace_cwd, copilot_workspace_summary,
    CODEX_SESSION_INDEX, COPILOT_WORKSPACE_FILE, TRANSCRIPT_EXTENSION,
};
use crate::agents::resume::{first_line, recorded_cwd, rollout_files, CODEX_SESSIONS_DIRECTORY};
use crate::agents::running_model::copilot_session_root;
use crate::agents::{
    claude_directory, claude_projects_directory, codex_home, ID_CLAUDE, ID_CODEX, ID_COPILOT,
};
use crate::util::{
    encode_project, json_string_field, milliseconds_since_unix_epoch, now_milliseconds,
};

const BYTES_PER_MEGABYTE: u64 = 1024 * 1024;
const MILLISECONDS_PER_DAY: u64 = 24 * 60 * 60 * 1000;
/// A chat used this recently is never picked, whatever the limits say.
const PROTECTED_DAYS: u64 = 7;
/// The folders under `~/.claude` that hold one sub-folder per conversation id.
const CLAUDE_PER_CHAT_FOLDERS: [&str; 2] = ["file-history", "session-env"];
/// Copilot's event log, rewritten on every turn - the best "last used" stamp.
const COPILOT_EVENT_LOG: &str = "events.jsonl";
/// The project key of a chat whose project couldn't be read.
const UNKNOWN_PROJECT: &str = "";
/// How many of the chats a cleanup deletes are named, largest first.
const LISTED_CHATS: usize = 100;
/// How much of a Claude transcript's start is read for the folder it ran in.
const TRANSCRIPT_HEAD_BYTES: u64 = 64 * 1024;
/// The field of a Codex rollout's first line naming its session.
const SESSION_ID_FIELD: &str = "session_id";

/// One recorded conversation, as storage sees it.
#[derive(Debug, Clone)]
struct Chat {
    agent_id: &'static str,
    /// Which project the chat belongs to, the same key across agents.
    project: String,
    /// Milliseconds since the Unix epoch.
    updated_at: u64,
    bytes: u64,
    paths: Vec<PathBuf>,
}

impl Chat {
    /// The chat's identity across calls: its first file's path, the one the
    /// confirmation hands back for a chat the user ticked to delete.
    fn id(&self) -> String {
        self.paths
            .first()
            .map(|path| path.to_string_lossy().into_owned())
            .unwrap_or_default()
    }
}

/// The user's caps, in bytes. `None` is no cap.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
struct Limits {
    per_chat: Option<u64>,
    total: Option<u64>,
}

impl Limits {
    fn from_megabytes(per_chat: Option<u32>, total: Option<u32>) -> Self {
        let to_bytes = |megabytes: u32| u64::from(megabytes) * BYTES_PER_MEGABYTE;
        Self {
            per_chat: per_chat.map(to_bytes),
            total: total.map(to_bytes),
        }
    }

    fn saved() -> Self {
        let prefs = crate::workspace::load().prefs;
        Self::from_megabytes(
            prefs.chat_size_limit_megabytes,
            prefs.chat_storage_limit_megabytes,
        )
    }
}

/// One agent's share of the disk.
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct AgentStorage {
    agent_id: &'static str,
    bytes: u64,
    chat_count: usize,
}

/// One chat over the limits, as the confirmation names it.
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct DeletedChat {
    /// What the confirmation hands back to delete this chat.
    id: String,
    agent_id: &'static str,
    /// The agent's own title for it, when it gave one.
    title: Option<String>,
    /// The folder it ran in, when its files say.
    project: Option<String>,
    bytes: u64,
    /// Milliseconds since the Unix epoch.
    updated_at: u64,
}

/// The chats over the limits, which a cleanup could remove: the totals, and
/// the ones most worth deleting by name.
#[derive(Debug, Clone, Default, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct Cleanup {
    bytes: u64,
    chat_count: usize,
    chats: Vec<DeletedChat>,
}

/// The chats' disk use, and what the given limits would delete from it.
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ChatStorage {
    bytes: u64,
    chat_count: usize,
    agents: Vec<AgentStorage>,
    cleanup: Cleanup,
}

/// Every chat's disk use, and which chats `sizeLimitMegabytes` /
/// `storageLimitMegabytes` put over the limits - the settings card shows both
/// before a limit is saved.
// `async` plus `spawn_blocking`: it walks tens of thousands of files.
#[tauri::command]
pub async fn chat_storage(
    size_limit_megabytes: Option<u32>,
    storage_limit_megabytes: Option<u32>,
) -> Result<ChatStorage, String> {
    tauri::async_runtime::spawn_blocking(move || {
        measure(Limits::from_megabytes(
            size_limit_megabytes,
            storage_limit_megabytes,
        ))
    })
    .await
    .map_err(|error| error.to_string())
}

/// Delete the chats the user ticked (`ids`), then measure again against the
/// saved limits. Only a chat the saved limits put over them can go: an id is
/// matched against that plan, never taken as a path to delete, so nothing
/// else on disk is reachable through this command.
#[tauri::command]
pub async fn chat_storage_delete(ids: Vec<String>) -> Result<ChatStorage, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let limits = Limits::saved();
        let ticked: HashSet<String> = ids.into_iter().collect();
        let chats = recorded_chats();
        for index in chats_to_delete(&chats, limits, now_milliseconds()) {
            if ticked.contains(&chats[index].id()) {
                delete_chat(&chats[index]);
            }
        }
        measure(limits)
    })
    .await
    .map_err(|error| error.to_string())
}

fn measure(limits: Limits) -> ChatStorage {
    let chats = recorded_chats();
    let now = now_milliseconds();
    let deleted = chats_to_delete(&chats, limits, now);
    storage_of(&chats, &deleted, now)
}

/// Remove every file and folder of `chat`. One that is already gone is fine;
/// one that can't be removed (still held open) is reported and stays over the
/// limit, to be ticked again.
fn delete_chat(chat: &Chat) {
    for path in &chat.paths {
        let Ok(metadata) = std::fs::symlink_metadata(path) else {
            continue;
        };
        let removed = if metadata.is_dir() {
            std::fs::remove_dir_all(path)
        } else {
            std::fs::remove_file(path)
        };
        if let Err(error) = removed {
            eprintln!("could not delete chat file {}: {error}", path.display());
        }
    }
}

fn storage_of(chats: &[Chat], deleted: &[usize], now: u64) -> ChatStorage {
    let mut agents: BTreeMap<&'static str, AgentStorage> = BTreeMap::new();
    for chat in chats {
        let agent = agents.entry(chat.agent_id).or_insert(AgentStorage {
            agent_id: chat.agent_id,
            bytes: 0,
            chat_count: 0,
        });
        agent.bytes += chat.bytes;
        agent.chat_count += 1;
    }
    let mut agents: Vec<AgentStorage> = agents.into_values().collect();
    agents.sort_by_key(|agent| std::cmp::Reverse(agent.bytes));

    ChatStorage {
        bytes: chats.iter().map(|chat| chat.bytes).sum(),
        chat_count: chats.len(),
        agents,
        cleanup: Cleanup {
            bytes: deleted.iter().map(|&index| chats[index].bytes).sum(),
            chat_count: deleted.len(),
            chats: named_chats(chats, deleted, now),
        },
    }
}

/// How much a chat is worth deleting: its size times how long it has sat
/// unused, so an old, heavy chat outranks both a big fresh one and an old
/// small one.
fn deletion_weight(chat: &Chat, now: u64) -> u128 {
    u128::from(chat.bytes) * u128::from(now.saturating_sub(chat.updated_at))
}

/// The `deleted` chats most worth deleting ([`deletion_weight`]), by name -
/// reading each one's title and folder off its files, so only the
/// [`LISTED_CHATS`] shown are read.
fn named_chats(chats: &[Chat], deleted: &[usize], now: u64) -> Vec<DeletedChat> {
    let mut largest: Vec<&Chat> = deleted.iter().map(|&index| &chats[index]).collect();
    largest.sort_by_key(|chat| std::cmp::Reverse(deletion_weight(chat, now)));
    largest.truncate(LISTED_CHATS);
    let codex_titles = codex_home()
        .map(|home| codex_titles(&home.join(CODEX_SESSION_INDEX)))
        .unwrap_or_default();

    largest
        .into_iter()
        .map(|chat| {
            let (title, project) = chat
                .paths
                .first()
                .map(|path| title_and_project(chat.agent_id, path, &codex_titles))
                .unwrap_or_default();
            DeletedChat {
                id: chat.id(),
                agent_id: chat.agent_id,
                title,
                project,
                bytes: chat.bytes,
                updated_at: chat.updated_at,
            }
        })
        .collect()
}

/// A chat's title and folder, off its first file: a Claude transcript, a
/// Codex rollout or a Copilot session folder.
fn title_and_project(
    agent_id: &str,
    path: &Path,
    codex_titles: &HashMap<String, String>,
) -> (Option<String>, Option<String>) {
    match agent_id {
        ID_CLAUDE => {
            let head = std::fs::File::open(path).ok().and_then(|file| {
                let mut head = String::new();
                file.take(TRANSCRIPT_HEAD_BYTES)
                    .read_to_string(&mut head)
                    .ok()?;
                Some(head)
            });
            (
                claude_transcript_title(path),
                head.and_then(|head| recorded_cwd(&head)),
            )
        }
        ID_CODEX => {
            let line = first_line(path).unwrap_or_default();
            let title = json_string_field(&line, SESSION_ID_FIELD)
                .and_then(|id| codex_titles.get(&id).cloned());
            (title, recorded_cwd(&line))
        }
        ID_COPILOT => {
            let workspace = path.join(COPILOT_WORKSPACE_FILE);
            (
                copilot_workspace_summary(&workspace),
                copilot_workspace_cwd(&workspace),
            )
        }
        _ => (None, None),
    }
}

/// Which chats the limits delete, as indices into `chats`, in the order they
/// go. First every chat over the size cap; then, while all chats together are
/// over the total, round after round of each project's oldest remaining chat,
/// the oldest of a round first. Protected chats (a project's newest, or one
/// used in the last [`PROTECTED_DAYS`]) are never chosen.
fn chats_to_delete(chats: &[Chat], limits: Limits, now: u64) -> Vec<usize> {
    let protected_since = now.saturating_sub(PROTECTED_DAYS * MILLISECONDS_PER_DAY);
    let mut by_project: BTreeMap<&str, Vec<usize>> = BTreeMap::new();
    for (index, chat) in chats.iter().enumerate() {
        by_project.entry(&chat.project).or_default().push(index);
    }
    let deletable: Vec<Vec<usize>> = by_project
        .into_values()
        .map(|mut indices| {
            indices.sort_by_key(|&index| chats[index].updated_at);
            indices.pop();
            indices.retain(|&index| chats[index].updated_at < protected_since);
            indices
        })
        .collect();

    let mut chosen = Vec::new();
    let mut is_chosen = HashSet::new();
    if let Some(per_chat) = limits.per_chat {
        let mut oversized: Vec<usize> = deletable
            .iter()
            .flatten()
            .copied()
            .filter(|&index| chats[index].bytes > per_chat)
            .collect();
        oversized.sort_by_key(|&index| chats[index].updated_at);
        is_chosen.extend(oversized.iter().copied());
        chosen.extend(oversized);
    }

    let Some(total) = limits.total else {
        return chosen;
    };
    let mut remaining: u64 = chats.iter().map(|chat| chat.bytes).sum::<u64>()
        - chosen.iter().map(|&index| chats[index].bytes).sum::<u64>();
    let queues: Vec<Vec<usize>> = deletable
        .into_iter()
        .map(|indices| {
            indices
                .into_iter()
                .filter(|index| !is_chosen.contains(index))
                .collect()
        })
        .collect();
    let rounds = queues.iter().map(Vec::len).max().unwrap_or(0);
    for round in 0..rounds {
        let mut this_round: Vec<usize> = queues
            .iter()
            .filter_map(|queue| queue.get(round).copied())
            .collect();
        this_round.sort_by_key(|&index| chats[index].updated_at);
        for index in this_round {
            if remaining <= total {
                return chosen;
            }
            remaining -= chats[index].bytes;
            chosen.push(index);
        }
    }
    chosen
}

/// Every chat of every agent PADE knows the files of.
fn recorded_chats() -> Vec<Chat> {
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

#[cfg(test)]
mod tests;
