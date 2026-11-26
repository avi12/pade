use crate::agents::storage::{chats_to_delete, storage_of, Chat, Limits, MILLISECONDS_PER_DAY};
use std::path::PathBuf;

const OLDEST_AGE: u64 = 90;
const LARGE_SIZE: u64 = 500;
const OLDER_AGE: u64 = 60;
const THREE_HUNDRED_SIZE: u64 = 300;
const MEDIUM_SIZE: u64 = 100;
const RECENT_AGE: u64 = 30;
const TINY_SIZE: u64 = 10;
const NEWEST_AGE: u64 = 3;
const HUGE_SIZE: u64 = 900;
const STALE_AGE: u64 = 400;
const SECOND_STALE_AGE: u64 = 390;
const THIRD_STALE_AGE: u64 = 380;
const FRESH_AGE: u64 = 10;
const BUSY_OLD_AGE: u64 = 200;
const SMALL_SIZE: u64 = 50;
const OVERSIZED: u64 = 1000;
const MIDDLE_AGE: u64 = 300;

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
    let chats = [
        chat("a", OLDEST_AGE, LARGE_SIZE),
        chat("a", OLDER_AGE, LARGE_SIZE),
        chat("a", 1, LARGE_SIZE),
    ];
    assert_eq!(
        chats_to_delete(&chats, Limits::default(), NOW),
        Vec::<usize>::new()
    );
}

#[test]
fn a_chat_over_the_size_cap_goes() {
    let chats = [
        chat("a", OLDEST_AGE, THREE_HUNDRED_SIZE),
        chat("a", OLDER_AGE, MEDIUM_SIZE),
        chat("a", RECENT_AGE, TINY_SIZE),
    ];
    assert_eq!(
        chats_to_delete(&chats, limits(Some(200), None), NOW),
        vec![0]
    );
}

#[test]
fn a_projects_newest_chat_and_last_weeks_chats_stay() {
    let chats = [
        chat("a", NEWEST_AGE, HUGE_SIZE),
        chat("a", OLDER_AGE, HUGE_SIZE),
        chat("b", OLDEST_AGE, HUGE_SIZE),
    ];
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
        chat("old", STALE_AGE, MEDIUM_SIZE),
        chat("old", SECOND_STALE_AGE, MEDIUM_SIZE),
        chat("old", THIRD_STALE_AGE, MEDIUM_SIZE),
        chat("old", FRESH_AGE, MEDIUM_SIZE),
        chat("busy", BUSY_OLD_AGE, MEDIUM_SIZE),
        chat("busy", MEDIUM_SIZE, MEDIUM_SIZE),
        chat("busy", FRESH_AGE, MEDIUM_SIZE),
    ];
    assert_eq!(
        chats_to_delete(&chats, limits(None, Some(400)), NOW),
        vec![0, 4, 1]
    );
}

#[test]
fn the_total_stops_once_under_the_cap() {
    let chats = [
        chat("a", OLDEST_AGE, MEDIUM_SIZE),
        chat("a", OLDER_AGE, MEDIUM_SIZE),
        chat("a", RECENT_AGE, MEDIUM_SIZE),
    ];
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
    let chats = [
        chat("a", OLDEST_AGE, MEDIUM_SIZE),
        chat("a", OLDER_AGE, LARGE_SIZE),
        chat("a", RECENT_AGE, MEDIUM_SIZE),
    ];
    assert_eq!(
        chats_to_delete(&chats, limits(Some(200), Some(200)), NOW),
        vec![1]
    );
}

#[test]
fn storage_sums_each_agent_and_the_cleanup() {
    let mut codex = chat("a", OLDEST_AGE, SMALL_SIZE);
    codex.agent_id = "codex";
    let chats = [
        chat("a", OLDEST_AGE, THREE_HUNDRED_SIZE),
        codex,
        chat("a", 1, TINY_SIZE),
    ];
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
        chat("a", FRESH_AGE, OVERSIZED),
        chat("b", STALE_AGE, TINY_SIZE),
        chat("c", MIDDLE_AGE, LARGE_SIZE),
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
