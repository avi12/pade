use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Mutex;

use tokio::sync::mpsc;

use super::connection::Connection;

#[test]
fn a_closed_connection_reports_each_known_session_once() {
    let (outbound, _queue) = mpsc::channel(1);
    let connection = Connection {
        outbound,
        pending: Mutex::new(HashMap::new()),
        next_request_id: AtomicU64::new(1),
        is_open: AtomicBool::new(true),
        known_sessions: Mutex::new(HashMap::new()),
    };
    connection.note_session("a", "main");
    connection.note_session("b", "w-1");
    connection.note_session("b", "w-2");
    connection.forget_session("a");
    assert_eq!(
        connection.close(),
        HashMap::from([("b".to_string(), "w-2".to_string())])
    );
    assert!(connection.close().is_empty());
    assert!(!connection.is_open.load(Ordering::Acquire));
}
