use super::order::{cycled_index, order_key, window_sort_key, CycleDirection};
use super::{labels_to_recreate, reserve_sequences_past, LaunchMode, NEXT_WINDOW_SEQUENCE};

#[test]
fn only_missing_windows_other_than_main_are_recreated_once_each_in_order() {
    let owners = ["w-10", "main", "w-2", "w-2", "w-3", "dashboard"];
    let live = vec!["main".to_string(), "w-3".to_string()];
    assert_eq!(
        labels_to_recreate(owners.into_iter(), &live),
        ["w-2", "w-10", "dashboard"]
    );
    crate::test_support::assert_empty(&labels_to_recreate(["main"].into_iter(), &[]));
}

#[test]
fn new_windows_number_past_every_recreated_one() {
    reserve_sequences_past(&["w-40".to_string(), "dashboard".to_string()]);
    assert!(NEXT_WINDOW_SEQUENCE.load(std::sync::atomic::Ordering::Relaxed) > 40);
}

/// Sort the live window labels the way `window_list`/`window_focus_relative` do,
/// given the user's explicit order - the behaviour both commands share.
fn sorted(live: &[&str], order: &[&str]) -> Vec<String> {
    let order: Vec<String> = order.iter().map(|label| (*label).to_string()).collect();
    let mut labels: Vec<String> = live.iter().map(|label| (*label).to_string()).collect();
    labels.sort_by_cached_key(|label| window_sort_key(label, &order));
    labels
}

#[test]
fn explicit_order_wins_over_creation_order() {
    // Creation order alone would be main, w-1, w-2; the explicit order overrides it.
    assert_eq!(
        sorted(&["main", "w-1", "w-2"], &["w-2", "main", "w-1"]),
        ["w-2", "main", "w-1"]
    );
}

#[test]
fn unknown_windows_fall_back_to_creation_order() {
    // Only w-2 is explicitly ordered; the rest follow in creation order - and
    // w-2 (numeric 2) precedes w-10, which lexicographic sorting would misplace.
    assert_eq!(
        sorted(&["main", "w-2", "w-10"], &["w-2"]),
        ["w-2", "main", "w-10"]
    );
}

#[test]
fn removed_label_is_ignored() {
    // w-9 sits in the explicit order but is no longer live, so it never appears;
    // the remaining live windows keep the explicit order.
    assert_eq!(
        sorted(&["main", "w-1"], &["w-1", "w-9", "main"]),
        ["w-1", "main"]
    );
}

#[test]
fn creation_order_key_parses_sequence_numerically() {
    assert_eq!(order_key("main"), 0);
    assert!(order_key("w-2") < order_key("w-10"));
}

#[test]
fn cycling_wraps_around_at_both_ends() {
    assert_eq!(cycled_index(2, 3, CycleDirection::Next), 0);
    assert_eq!(cycled_index(0, 3, CycleDirection::Previous), 2);
    assert_eq!(cycled_index(1, 3, CycleDirection::Next), 2);
}

#[test]
fn a_path_always_opens_that_project_whatever_the_mode() {
    assert_eq!(LaunchMode::resolve("temp", None).query(), "w=temp");
    assert_eq!(LaunchMode::resolve("empty", None).query(), "w=empty");
    assert_eq!(LaunchMode::resolve("anything", None).query(), "w=empty");
    assert_eq!(
        LaunchMode::resolve("temp", Some(r"C:\my project")).query(),
        "w=open&path=C%3A%5Cmy%20project"
    );
}

#[test]
fn a_handoff_window_boots_its_own_route_only_with_a_project() {
    assert_eq!(
        LaunchMode::resolve("handoff", Some(r"C:\work")).query(),
        "w=handoff&path=C%3A%5Cwork"
    );
    assert_eq!(LaunchMode::resolve("handoff", None).query(), "w=empty");
}

#[test]
fn the_dashboard_always_takes_its_one_label() {
    let dashboard = LaunchMode::resolve("dashboard", None);
    assert_eq!(dashboard.query(), "w=dashboard");
    assert_eq!(dashboard.label(), dashboard.label());
    assert!(LaunchMode::resolve("empty", None).label().starts_with("w-"));
    assert!(dashboard.has_browser_zoom());
    assert!(!LaunchMode::resolve("empty", None).has_browser_zoom());
}
