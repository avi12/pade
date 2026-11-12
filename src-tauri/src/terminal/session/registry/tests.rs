use super::*;
use std::sync::mpsc;

#[test]
fn a_moved_owner_is_seen_by_every_clone() {
    let owner = SessionOwner::new("w-1".to_string());
    let pump_view = owner.clone();
    owner.set("w-2").expect("set owner");
    assert!(pump_view.is("w-2"));
    assert!(!pump_view.is("w-1"));
    assert_eq!(pump_view.label(), "w-2");
}

#[test]
fn exit_waits_for_a_teardown_still_running() {
    let teardowns = Arc::new(Teardowns::default());
    let teardown = teardowns.begin();
    let is_finished = Arc::new(std::sync::atomic::AtomicBool::new(false));
    let worker = {
        let is_finished = Arc::clone(&is_finished);
        std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(100));
            is_finished.store(true, std::sync::atomic::Ordering::SeqCst);
            drop(teardown);
        })
    };
    teardowns.wait_for_all(Duration::from_secs(5));
    assert!(is_finished.load(std::sync::atomic::Ordering::SeqCst));
    worker.join().expect("worker thread");
}

#[test]
fn exit_never_waits_past_its_timeout() {
    let teardowns = Arc::new(Teardowns::default());
    let _stuck = teardowns.begin();
    let started = std::time::Instant::now();
    teardowns.wait_for_all(Duration::from_millis(50));
    assert!(started.elapsed() < Duration::from_secs(2));
}

#[test]
fn errors_keep_the_messages_the_frontend_has_always_shown() {
    assert_eq!(
        SessionError::Foreign.to_string(),
        "PTY belongs to another window"
    );
    assert_eq!(
        SessionError::Missing { id: "a".into() }.to_string(),
        "no live session a to move"
    );
    assert_eq!(
        serde_json::to_value(SessionError::Missing { id: "a".into() }).expect("serializable"),
        serde_json::json!({ "kind": "missing", "id": "a" })
    );
}

/// What the engine handed on, in order - a sink with no transport.
#[derive(Debug, PartialEq)]
enum Event {
    Output(String),
    Exited,
}

struct Collector(Mutex<mpsc::Sender<Event>>);

impl SessionEvents for Collector {
    fn output(&self, _owner: &str, _id: &str, data: &str, _sequence: u64) {
        if let Ok(sender) = self.0.lock() {
            let _ = sender.send(Event::Output(data.to_string()));
        }
    }

    fn exited(&self, _owner: &str, _id: &str) {
        if let Ok(sender) = self.0.lock() {
            let _ = sender.send(Event::Exited);
        }
    }
}

/// The whole engine without a pipe: a real PTY runs a real command, its
/// output reaches the sink and the history, the owner check holds, and the
/// session drops out of the registry once the process exits on its own.
#[cfg(windows)]
#[test]
fn a_real_command_streams_into_history_and_is_reaped() {
    let (sender, receiver) = mpsc::channel();
    let registry = SessionRegistry::new(Arc::new(Collector(Mutex::new(sender))));
    let launch = SessionLaunch {
        command: Some("cmd".into()),
        columns: 80,
        rows: 24,
        arguments: vec!["/c".into(), "echo pade-engine-test".into()],
        ..SessionLaunch::default()
    };
    registry.spawn("s1", "main", &launch).expect("spawned");
    assert_eq!(
        registry.history("other", "s1"),
        Err(SessionError::Foreign),
        "another window is refused"
    );

    let mut said = String::new();
    let deadline = std::time::Instant::now() + Duration::from_secs(20);
    loop {
        let remaining = deadline.saturating_duration_since(std::time::Instant::now());
        match receiver.recv_timeout(remaining) {
            Ok(Event::Output(data)) => said.push_str(&data),
            Ok(Event::Exited) | Err(_) => break,
        }
    }
    assert!(said.contains("pade-engine-test"), "output was: {said:?}");

    while registry.count() > 0 && std::time::Instant::now() < deadline {
        std::thread::sleep(CHILD_POLL_INTERVAL);
    }
    assert_eq!(registry.count(), 0, "the exited session is reaped");
}
