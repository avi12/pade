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

/// The session file every test session ran in this project from.
const SESSION_FILE_STEM: &str = "in-project";

fn scopes_of(path: &Path) -> Vec<ChangeScope> {
    changed_scopes(path, &stores(), &listed(), |path, _| {
        let is_in_project = path
            .to_str()
            .is_some_and(|text| text.contains(SESSION_FILE_STEM));
        is_in_project.then(|| PROJECT.to_string())
    })
}

fn codex_rollout(name: &str) -> PathBuf {
    root()
        .join(".codex")
        .join("sessions")
        .join("2026")
        .join("10")
        .join("25")
        .join(name)
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
fn a_codex_rollout_names_the_project_it_ran_in() {
    let rollout = codex_rollout(&format!("rollout-{SESSION_FILE_STEM}.jsonl"));
    assert_eq!(
        scopes_of(&rollout),
        vec![ChangeScope::Project(PROJECT.to_string())]
    );
}

#[test]
fn a_gone_codex_rollout_concerns_every_project() {
    assert_eq!(
        scopes_of(&codex_rollout("rollout-gone.jsonl")),
        vec![ChangeScope::Every]
    );
}

#[test]
fn codex_title_index_concerns_every_project_and_its_logs_are_noise() {
    let index = root().join(".codex").join(CODEX_SESSION_INDEX);
    assert_eq!(scopes_of(&index), vec![ChangeScope::Every]);
    let log = root().join(".codex").join("history.jsonl");
    assert_eq!(scopes_of(&log), Vec::new());
}

#[test]
fn a_copilot_workspace_names_its_project_and_a_session_folder_every_one() {
    let session = root().join(".copilot").join("session-state").join("id-1");
    assert_eq!(scopes_of(&session), vec![ChangeScope::Every]);
    let in_project = root()
        .join(".copilot")
        .join("session-state")
        .join(SESSION_FILE_STEM);
    assert_eq!(
        scopes_of(&in_project.join(COPILOT_WORKSPACE_FILE)),
        vec![ChangeScope::Project(PROJECT.to_string())]
    );
    assert_eq!(scopes_of(&session.join("events.jsonl")), Vec::new());
}

#[test]
fn a_session_in_an_unlisted_project_is_noise() {
    let scopes = session_scopes(
        Path::new("C:/home/.codex/sessions/rollout.jsonl"),
        Some("C:/never-listed".to_string()),
        &listed(),
    );
    assert_eq!(scopes, Vec::new());
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
        Emission::Deadline(last + EMISSION_INTERVAL)
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
