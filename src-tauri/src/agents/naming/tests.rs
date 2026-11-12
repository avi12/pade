use super::{dominant_stem, extract_name, sanitize};

#[test]
fn sanitize_kebabs_a_titlecase_phrase() {
    assert_eq!(sanitize("Brave Otter!").as_deref(), Some("brave-otter"));
}

#[test]
fn sanitize_keeps_an_already_kebab_name() {
    assert_eq!(sanitize("brave-otter").as_deref(), Some("brave-otter"));
}

#[test]
fn sanitize_uses_only_the_first_line() {
    assert_eq!(
        sanitize("tidy-name\nwith an explanation").as_deref(),
        Some("tidy-name")
    );
}

#[test]
fn sanitize_caps_at_four_words() {
    assert_eq!(
        sanitize("one two three four five").as_deref(),
        Some("one-two-three-four")
    );
}

#[test]
fn sanitize_collapses_separator_runs() {
    assert_eq!(sanitize("hello___world!!").as_deref(), Some("hello-world"));
}

#[test]
fn sanitize_caps_the_length_at_forty() {
    assert_eq!(sanitize(&"a".repeat(60)), Some("a".repeat(40)));
}

#[test]
fn sanitize_rejects_candidates_too_short_to_mean_anything() {
    assert_eq!(sanitize("x"), None);
    assert_eq!(sanitize("!!!"), None);
    assert_eq!(sanitize(""), None);
}

#[test]
fn extract_name_prefers_a_bare_token_line() {
    assert_eq!(
        extract_name("Sure! A good name would be:\n\nbrave-otter\n").as_deref(),
        Some("brave-otter")
    );
}

#[test]
fn extract_name_falls_back_to_the_last_nonempty_line() {
    assert_eq!(
        extract_name("The best name is:\nMy Cool Project\n").as_deref(),
        Some("My Cool Project")
    );
}

#[test]
fn extract_name_yields_nothing_for_blank_output() {
    assert_eq!(extract_name("\n  \n"), None);
}

#[test]
fn readable_transcript_collapses_repeated_repaint_frames() {
    // A fullscreen agent repaints the same frame each spinner tick; only the
    // substantive line should survive once, chrome deduped to a single copy.
    let painted =
        "⠋ Working\nSearch Tel Aviv startups\n⠙ Working\nSearch Tel Aviv startups\n⠹ Working";
    assert_eq!(
        super::readable_transcript(painted),
        "⠋ Working\nSearch Tel Aviv startups\n⠙ Working\n⠹ Working"
    );
}

#[test]
fn dominant_stem_picks_the_most_frequent_meaningful_stem() {
    let files = ["widget.rs", "widget.toml", "readme.md", "index.ts"].map(str::to_string);
    assert_eq!(dominant_stem(&files).as_deref(), Some("widget"));
}

#[test]
fn dominant_stem_ignores_noise_only_file_sets() {
    let files = ["index.ts", "main.rs", "readme.md"].map(str::to_string);
    assert_eq!(dominant_stem(&files), None);
}
