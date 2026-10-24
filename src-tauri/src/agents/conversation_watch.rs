//! Tells the windows the moment a project's recorded conversations may have
//! changed - a conversation started, renamed (`/rename`, Claude's generated
//! title), grown, or replaced by a handoff's successor - so the sidebar's
//! `/resume` lists stay live instead of waiting for a re-read.
//!
//! One watcher for the whole app sees the agents' own stores:
//!
//!   * Claude Code - `~/.claude/projects`, one folder per encoded project, so a
//!     change names its project through the projects the windows have listed;
//!   * Codex - its `sessions` tree and `session_index.jsonl` (titles), which
//!     name no project in the path, so they announce a change to every one;
//!   * Copilot - its session-state folders and their `workspace.yaml`, likewise.
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
    CODEX_SESSION_INDEX, COPILOT_WORKSPACE_FILE, TRANSCRIPT_EXTENSION,
};
use crate::agents::resume::{CODEX_SESSIONS_DIRECTORY, ROLLOUT_EXTENSION};
use crate::agents::running_model::copilot_session_root;
use crate::agents::{claude_projects_directory, codex_home};
use crate::util::encode_project;

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
pub fn init(app: &AppHandle) {
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
            for scope in changed_scopes(path, &routes, &listed) {
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
/// subagent's transcript, Codex's logs, Copilot's per-turn event log).
fn changed_scopes(
    path: &Path,
    stores: &[(PathBuf, ConversationStore)],
    listed: &HashSet<String>,
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
    let is_every = match store {
        ConversationStore::ClaudeProjects => {
            return claude_scopes(&parts, listed);
        }
        ConversationStore::CodexSessions => has_extension(path, ROLLOUT_EXTENSION),
        ConversationStore::CodexHome => parts == [CODEX_SESSION_INDEX],
        ConversationStore::CopilotSessions => {
            matches!(parts.as_slice(), [_] | [_, COPILOT_WORKSPACE_FILE])
        }
    };
    if is_every {
        vec![ChangeScope::Every]
    } else {
        Vec::new()
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
mod tests {
    use super::*;

    const PROJECT: &str = "C:/repositories/pade";

    fn root() -> PathBuf {
        PathBuf::from("C:/home")
    }

    fn stores() -> Vec<(PathBuf, ConversationStore)> {
        let codex = root().join(".codex");
        vec![
            (
                root().join(".claude").join("projects"),
                ConversationStore::ClaudeProjects,
            ),
            (codex.join("sessions"), ConversationStore::CodexSessions),
            (codex, ConversationStore::CodexHome),
            (
                root().join(".copilot").join("session-state"),
                ConversationStore::CopilotSessions,
            ),
        ]
    }

    fn listed() -> HashSet<String> {
        HashSet::from([PROJECT.to_string(), "C:/elsewhere".to_string()])
    }

    fn scopes_of(path: &Path) -> Vec<ChangeScope> {
        changed_scopes(path, &stores(), &listed())
    }

    fn claude_folder() -> PathBuf {
        root()
            .join(".claude")
            .join("projects")
            .join(encode_project(PROJECT))
    }

    #[test]
    fn a_claude_transcript_names_its_listed_project() {
        assert_eq!(
            scopes_of(&claude_folder().join("abc.jsonl")),
            vec![ChangeScope::Project(PROJECT.to_string())]
        );
    }

    #[test]
    fn a_new_claude_project_folder_names_its_listed_project() {
        assert_eq!(
            scopes_of(&claude_folder()),
            vec![ChangeScope::Project(PROJECT.to_string())]
        );
    }

    #[test]
    fn a_subagent_transcript_or_an_unlisted_project_is_noise() {
        let subagent = claude_folder()
            .join("abc")
            .join("subagents")
            .join("agent.jsonl");
        assert_eq!(scopes_of(&subagent), Vec::new());
        let unlisted = root()
            .join(".claude")
            .join("projects")
            .join(encode_project("C:/never-listed"))
            .join("abc.jsonl");
        assert_eq!(scopes_of(&unlisted), Vec::new());
    }

    #[test]
    fn codex_rollouts_and_its_title_index_concern_every_project() {
        let rollout = root()
            .join(".codex")
            .join("sessions")
            .join("2026")
            .join("10")
            .join("25")
            .join("rollout-1.jsonl");
        assert_eq!(scopes_of(&rollout), vec![ChangeScope::Every]);
        let index = root().join(".codex").join(CODEX_SESSION_INDEX);
        assert_eq!(scopes_of(&index), vec![ChangeScope::Every]);
        let log = root().join(".codex").join("history.jsonl");
        assert_eq!(scopes_of(&log), Vec::new());
    }

    #[test]
    fn a_copilot_session_folder_or_workspace_concerns_every_project() {
        let session = root().join(".copilot").join("session-state").join("id-1");
        assert_eq!(scopes_of(&session), vec![ChangeScope::Every]);
        assert_eq!(
            scopes_of(&session.join(COPILOT_WORKSPACE_FILE)),
            vec![ChangeScope::Every]
        );
        assert_eq!(scopes_of(&session.join("events.jsonl")), Vec::new());
    }

    #[test]
    fn a_path_outside_every_store_is_noise() {
        assert_eq!(scopes_of(&root().join("notes.jsonl")), Vec::new());
    }

    #[test]
    fn the_first_change_is_announced_at_once() {
        let now = Instant::now();
        assert_eq!(next_emission(None, now), Emission::Now);
        assert_eq!(
            next_emission(Some(now), now + EMISSION_INTERVAL),
            Emission::Now
        );
    }

    #[test]
    fn a_change_within_the_interval_waits_for_its_end() {
        let last = Instant::now();
        let soon = last + EMISSION_INTERVAL / 4;
        assert_eq!(
            next_emission(Some(last), soon),
            Emission::At(last + EMISSION_INTERVAL)
        );
    }

    #[test]
    fn a_burst_folds_into_one_trailing_announcement_per_scope() {
        let start = Instant::now();
        let project = ChangeScope::Project(PROJECT.to_string());
        let mut throttle = Throttle::default();
        assert_eq!(throttle.note(project.clone(), start), Some(project.clone()));
        for step in 1..5 {
            let during = start + EMISSION_INTERVAL / 10 * step;
            assert_eq!(throttle.note(project.clone(), during), None);
        }
        assert_eq!(
            throttle.note(ChangeScope::Every, start),
            Some(ChangeScope::Every)
        );
        assert_eq!(throttle.next_deadline(), Some(start + EMISSION_INTERVAL));
        assert_eq!(throttle.due(start + EMISSION_INTERVAL / 2), Vec::new());
        assert_eq!(throttle.due(start + EMISSION_INTERVAL), vec![project]);
        assert_eq!(throttle.next_deadline(), None);
    }

    #[test]
    fn a_quiet_scope_is_forgotten_and_announced_at_once_again() {
        let start = Instant::now();
        let mut throttle = Throttle::default();
        throttle.note(ChangeScope::Every, start);
        let later = start + EMISSION_INTERVAL * 2;
        assert_eq!(throttle.due(later), Vec::new());
        assert!(throttle.last_emitted.is_empty());
        assert_eq!(
            throttle.note(ChangeScope::Every, later),
            Some(ChangeScope::Every)
        );
    }
}
