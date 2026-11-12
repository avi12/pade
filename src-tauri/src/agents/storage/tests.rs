use super::{chats_to_delete, storage_of, Chat, Limits, MILLISECONDS_PER_DAY};
use std::path::PathBuf;

const NOW: u64 = 1000 * MILLISECONDS_PER_DAY;

fn chat(project: &str, days_ago: u64, bytes: u64) -> Chat {
    Chat {
        agent_id: "claude",
        project: project.to_string(),
        updated_at: NOW - days_ago * MILLISECONDS_PER_DAY,
        bytes,
        paths: vec![PathBuf::from(format!("{project}-{days_ago}-{bytes}"))],
    }
}

fn limits(per_chat: Option<u64>, total: Option<u64>) -> Limits {
    Limits { per_chat, total }
}

#[test]
fn no_limits_delete_nothing() {
    let chats = [chat("a", 90, 500), chat("a", 60, 500), chat("a", 1, 500)];
    assert_eq!(
        chats_to_delete(&chats, Limits::default(), NOW),
        Vec::<usize>::new()
    );
}

#[test]
fn a_chat_over_the_size_cap_goes() {
    let chats = [chat("a", 90, 300), chat("a", 60, 100), chat("a", 30, 10)];
    assert_eq!(
        chats_to_delete(&chats, limits(Some(200), None), NOW),
        vec![0]
    );
}

#[test]
fn a_projects_newest_chat_and_last_weeks_chats_stay() {
    let chats = [chat("a", 3, 900), chat("a", 60, 900), chat("b", 90, 900)];
    assert_eq!(
        chats_to_delete(&chats, limits(Some(1), Some(0)), NOW),
        vec![1]
    );
}

#[test]
fn the_total_takes_each_projects_oldest_in_turn() {
    // Project "old" holds the three oldest chats; a plain oldest-first
    // cleanup would empty it before "busy" gave anything up.
    let chats = [
        chat("old", 400, 100),
        chat("old", 390, 100),
        chat("old", 380, 100),
        chat("old", 10, 100),
        chat("busy", 200, 100),
        chat("busy", 100, 100),
        chat("busy", 10, 100),
    ];
    assert_eq!(
        chats_to_delete(&chats, limits(None, Some(400)), NOW),
        vec![0, 4, 1]
    );
}

#[test]
fn the_total_stops_once_under_the_cap() {
    let chats = [chat("a", 90, 100), chat("a", 60, 100), chat("a", 30, 100)];
    assert_eq!(
        chats_to_delete(&chats, limits(None, Some(250)), NOW),
        vec![0]
    );
    assert_eq!(
        chats_to_delete(&chats, limits(None, Some(300)), NOW),
        Vec::<usize>::new()
    );
}

#[test]
fn oversized_chats_count_toward_the_total() {
    let chats = [chat("a", 90, 100), chat("a", 60, 500), chat("a", 30, 100)];
    assert_eq!(
        chats_to_delete(&chats, limits(Some(200), Some(200)), NOW),
        vec![1]
    );
}

#[test]
fn storage_sums_each_agent_and_the_cleanup() {
    let mut codex = chat("a", 90, 50);
    codex.agent_id = "codex";
    let chats = [chat("a", 90, 300), codex, chat("a", 1, 10)];
    let storage = storage_of(&chats, &[0], NOW);
    assert_eq!(storage.bytes, 360);
    assert_eq!(storage.chat_count, 3);
    assert_eq!(storage.agents[0].agent_id, "claude");
    assert_eq!(storage.agents[0].bytes, 310);
    assert_eq!(storage.cleanup.bytes, 300);
    assert_eq!(storage.cleanup.chat_count, 1);
}

#[test]
fn the_list_leads_with_old_heavy_chats() {
    let chats = [
        chat("a", 10, 1000),
        chat("b", 400, 10),
        chat("c", 300, 500),
        chat("d", 1, 1),
    ];
    let storage = storage_of(&chats, &[0, 1, 2], NOW);
    let listed: Vec<u64> = storage
        .cleanup
        .chats
        .iter()
        .map(|chat| chat.bytes)
        .collect();
    assert_eq!(listed, vec![500, 1000, 10]);
}
