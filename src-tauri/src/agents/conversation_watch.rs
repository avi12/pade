//! Tells the windows the moment a project's recorded conversations may have
//! changed - a conversation started, renamed (`/rename`, Claude's generated
//! title), grown, or replaced by a handoff's successor - so the sidebar's
//! `/resume` lists stay live instead of waiting for a re-read.
//!
//! One watcher for the whole app sees the agents' own stores:
//!
//!   * Claude Code - `~/.claude/projects`, one folder per encoded project, so a
//!     change names its project through the projects the windows have listed;
//!   * Codex - its `sessions` tree, whose rollouts name their project by the
//!     `cwd` on their first line, and `session_index.jsonl` (titles), which
//!     names none, so it announces a change to every one;
//!   * Copilot - each `workspace.yaml` names its project by its `cwd`; a
//!     session folder appearing or going announces to every one.
//!
//! A transcript is appended on every token batch, so announcements are
//! throttled per project on their own thread: the first change goes out at
//! once, later ones within `EMISSION_INTERVAL` are folded into one more
//! announcement at the end of it, so the newest state is never missed.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError};
use std::sync::{LazyLock, Mutex, PoisonError};
use std::time::{Duration, Instant};

use notify::{Event, RecommendedWatcher, RecursiveMode, Watcher};
use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager};

use crate::agents::conversations::{
    codex_rollout_cwd, copilot_workspace_cwd, CODEX_SESSION_INDEX, COPILOT_WORKSPACE_FILE,
    TRANSCRIPT_EXTENSION,
};
use crate::agents::resume::{CODEX_SESSIONS_DIRECTORY, ROLLOUT_EXTENSION};
use crate::agents::running_model::copilot_session_root;
use crate::agents::{claude_projects_directory, codex_home};
use crate::util::{encode_project, same_path};

const CHANGED_EVENT: &str = "agent-conversations://changed";
/// The least time between two announcements for one project.
const EMISSION_INTERVAL: Duration = Duration::from_secs(1);

/// Every project a window has asked `agent_conversations` about, as it asked -
/// the only way back from Claude's lossy encoded folder name to a path.
static LISTED_PROJECTS: LazyLock<Mutex<HashSet<String>>> = LazyLock::new(Default::default);

/// Remember that a window lists `project`, so a change in its Claude folder
/// can be announced under the path the window knows it by.
pub(super) fn remember_listed_project(project: &str) {
    LISTED_PROJECTS
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .insert(project.to_string());
}

/// Watch every agent's conversation store and announce changes, throttled.
/// Best-effort: an agent that isn't installed has no folder to watch, and
/// without the watch the sidebar still re-reads when its chats change.
pub(crate) fn init(app: &AppHandle) {
    let stores = watched_stores();
    let routes = stores.clone();
    let (sender, receiver) = mpsc::channel();
    let emitter = app.clone();
    std::thread::spawn(move || announce_throttled(&receiver, &emitter));

    let watcher = notify::recommended_watcher(move |result: notify::Result<Event>| {
        let Ok(event) = result else { return };
        if event.kind.is_access() {
            return;
        }
        let listed = LISTED_PROJECTS
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone();
        for path in &event.paths {
            for scope in changed_scopes(path, &routes, &listed, session_cwd) {
                // The receiving thread lives as long as the app; a failed send
                // means it is shutting down, with nobody left to tell.
                if sender.send(scope).is_err() {
                    return;
                }
            }
        }
    });
    let Ok(mut watcher) = watcher else {
        return;
    };
    for (directory, store) in &stores {
        // Best-effort per store: an agent that isn't installed has no folder.
        let _ = watcher.watch(directory, store.recursive_mode());
    }
    app.manage(ConversationWatch {
        _watcher: Mutex::new(watcher),
    });
}

/// Keeps the watch alive for the app's lifetime; dropping it ends the watch.
struct ConversationWatch {
    _watcher: Mutex<RecommendedWatcher>,
}

/// Which projects a change concerns.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
enum ChangeScope {
    /// One listed project, by the path its window asked about.
    Project(String),
    /// A store that names no project in its paths - every listed one re-reads.
    Every,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct ChangedPayload {
    /// `None` for a change every listed project should re-read for.
    project: Option<String>,
}

impl From<ChangeScope> for ChangedPayload {
    fn from(scope: ChangeScope) -> Self {
        match scope {
            ChangeScope::Project(project) => Self {
                project: Some(project),
            },
            ChangeScope::Every => Self { project: None },
        }
    }
}

/// What one watched folder holds, so a changed path can be read correctly.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ConversationStore {
    /// Claude's `projects` folder: `<encoded project>/<id>.jsonl`.
    ClaudeProjects,
    /// Codex's `sessions` tree of dated folders of rollouts.
    CodexSessions,
    /// Codex's home, watched flat for `session_index.jsonl`.
    CodexHome,
    /// Copilot's session-state: `<id>/workspace.yaml`.
    CopilotSessions,
}

impl ConversationStore {
    fn recursive_mode(self) -> RecursiveMode {
        if self == Self::CodexHome {
            RecursiveMode::NonRecursive
        } else {
            RecursiveMode::Recursive
        }
    }
}

/// Every folder to watch. Codex's `sessions` sits inside its home, so it comes
/// first: a path is read by the first store that holds it.
fn watched_stores() -> Vec<(PathBuf, ConversationStore)> {
    let codex = codex_home();
    [
        (
            claude_projects_directory(),
            ConversationStore::ClaudeProjects,
        ),
        (
            codex
                .as_ref()
                .map(|home| home.join(CODEX_SESSIONS_DIRECTORY)),
            ConversationStore::CodexSessions,
        ),
        (codex, ConversationStore::CodexHome),
        (copilot_session_root(), ConversationStore::CopilotSessions),
    ]
    .into_iter()
    .filter_map(|(directory, store)| Some((directory?, store)))
    .collect()
}

/// Which projects a changed path concerns - none when it is noise (a
/// subagent's transcript, Codex's logs or oneshot runs, Copilot's per-turn
/// event log). `session_cwd` reads where a Codex rollout's or a Copilot
/// workspace's session ran.
fn changed_scopes(
    path: &Path,
    stores: &[(PathBuf, ConversationStore)],
    listed: &HashSet<String>,
    session_cwd: impl Fn(&Path, ConversationStore) -> Option<String>,
) -> Vec<ChangeScope> {
    let Some((directory, store)) = stores
        .iter()
        .find(|(directory, _)| path.starts_with(directory))
    else {
        return Vec::new();
    };
    let Ok(relative) = path.strip_prefix(directory) else {
        return Vec::new();
    };
    let parts: Vec<&str> = relative.iter().filter_map(|part| part.to_str()).collect();
    let is_session_file = match store {
        ConversationStore::ClaudeProjects => return claude_scopes(&parts, listed),
        ConversationStore::CodexHome if parts == [CODEX_SESSION_INDEX] => {
            return vec![ChangeScope::Every];
        }
        ConversationStore::CopilotSessions if parts.len() == 1 => {
            return vec![ChangeScope::Every];
        }
        ConversationStore::CodexHome => false,
        ConversationStore::CodexSessions => has_extension(path, ROLLOUT_EXTENSION),
        ConversationStore::CopilotSessions => parts.get(1) == Some(&COPILOT_WORKSPACE_FILE),
    };
    if !is_session_file {
        return Vec::new();
    }

    session_scopes(path, session_cwd(path, *store), listed)
}

/// The listed projects a session ran in. A session file that can't say is a
/// oneshot or one still being written - its next write says again - unless it
/// is gone, which every listed project re-reads for.
fn session_scopes(path: &Path, cwd: Option<String>, listed: &HashSet<String>) -> Vec<ChangeScope> {
    let Some(cwd) = cwd else {
        return if path.exists() {
            Vec::new()
        } else {
            vec![ChangeScope::Every]
        };
    };
    listed
        .iter()
        .filter(|project| same_path(project, &cwd))
        .map(|project| ChangeScope::Project(project.clone()))
        .collect()
}

/// Where a changed Codex rollout's or Copilot workspace's session ran.
fn session_cwd(path: &Path, store: ConversationStore) -> Option<String> {
    if store == ConversationStore::CodexSessions {
        codex_rollout_cwd(path)
    } else {
        copilot_workspace_cwd(path)
    }
}

/// A project folder appearing or going, or a transcript directly inside it,
/// speaks for every listed project that encodes to that folder's name.
fn claude_scopes(parts: &[&str], listed: &HashSet<String>) -> Vec<ChangeScope> {
    let folder = match parts {
        [folder] => folder,
        [folder, transcript] if has_extension(Path::new(transcript), TRANSCRIPT_EXTENSION) => {
            folder
        }
        _ => return Vec::new(),
    };
    listed
        .iter()
        .filter(|project| encode_project(project) == *folder)
        .map(|project| ChangeScope::Project(project.clone()))
        .collect()
}

fn has_extension(path: &Path, extension: &str) -> bool {
    path.extension().is_some_and(|found| found == extension)
}

/// When a change may be announced.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Emission {
    Now,
    At(Instant),
}

/// The throttle rule: at once unless the last announcement for the same scope
/// was under `EMISSION_INTERVAL` ago, else once that interval has passed.
fn next_emission(last_emitted: Option<Instant>, now: Instant) -> Emission {
    match last_emitted {
        Some(last) if now < last + EMISSION_INTERVAL => Emission::At(last + EMISSION_INTERVAL),
        _ => Emission::Now,
    }
}

/// The throttle's state per scope: when each was last announced, and which
/// are waiting for their interval to pass.
#[derive(Debug, Default)]
struct Throttle {
    last_emitted: HashMap<ChangeScope, Instant>,
    pending: HashMap<ChangeScope, Instant>,
}

impl Throttle {
    /// Note a change; `Some` when it should be announced right away.
    fn note(&mut self, scope: ChangeScope, now: Instant) -> Option<ChangeScope> {
        if self.pending.contains_key(&scope) {
            return None;
        }
        match next_emission(self.last_emitted.get(&scope).copied(), now) {
            Emission::Now => {
                self.last_emitted.insert(scope.clone(), now);
                Some(scope)
            }
            Emission::At(deadline) => {
                self.pending.insert(scope, deadline);
                None
            }
        }
    }

    /// The waiting scopes whose interval has passed, now announced - and the
    /// announcements old enough to no longer hold anything back are forgotten.
    fn due(&mut self, now: Instant) -> Vec<ChangeScope> {
        let due: Vec<ChangeScope> = self
            .pending
            .iter()
            .filter(|(_, deadline)| **deadline <= now)
            .map(|(scope, _)| scope.clone())
            .collect();
        for scope in &due {
            self.pending.remove(scope);
            self.last_emitted.insert(scope.clone(), now);
        }
        self.last_emitted
            .retain(|_, last| now < *last + EMISSION_INTERVAL);
        due
    }

    fn next_deadline(&self) -> Option<Instant> {
        self.pending.values().min().copied()
    }
}

/// The announcing thread: waits for a change or the next deadline, whichever
/// comes first, and emits what the throttle lets through.
fn announce_throttled(receiver: &Receiver<ChangeScope>, emitter: &AppHandle) {
    let mut throttle = Throttle::default();
    loop {
        let next_change = match throttle.next_deadline() {
            Some(deadline) => {
                receiver.recv_timeout(deadline.saturating_duration_since(Instant::now()))
            }
            None => receiver.recv().map_err(|_| RecvTimeoutError::Disconnected),
        };
        let now = Instant::now();
        match next_change {
            Ok(scope) => {
                if let Some(scope) = throttle.note(scope, now) {
                    announce(emitter, scope);
                }
            }
            Err(RecvTimeoutError::Timeout) => {}
            Err(RecvTimeoutError::Disconnected) => return,
        }
        for scope in throttle.due(now) {
            announce(emitter, scope);
        }
    }
}

fn announce(emitter: &AppHandle, scope: ChangeScope) {
    // Best-effort: a window that misses one still re-reads on its next change.
    let _ = emitter.emit(CHANGED_EVENT, ChangedPayload::from(scope));
}

#[cfg(test)]
mod tests;
