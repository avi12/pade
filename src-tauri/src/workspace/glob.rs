//! The one segment-aware glob matcher: `**` spans any number of `/`-separated
//! path segments, `*` and `?` stay within one. Shared by the `.gitignore`
//! matcher (`gitignore.rs`) and workspace member patterns (`members.rs`),
//! whose tools use the same semantics.

/// The pattern segment that spans any number of path segments.
const ANY_DEPTH_SEGMENT: &str = "**";

/// Whether glob `pattern` segments match `path` segments exactly: `**` spans
/// any number of segments, `*`/`?` stay within one.
pub(crate) fn segments_match<Segment: AsRef<str>>(pattern: &[Segment], path: &[&str]) -> bool {
    let Some((head, rest)) = pattern.split_first() else {
        return path.is_empty();
    };
    if head.as_ref() == ANY_DEPTH_SEGMENT {
        return (0..=path.len()).any(|skip| segments_match(rest, &path[skip..]));
    }
    path.split_first().is_some_and(|(first, tail)| {
        segment_matches(head.as_ref(), first) && segments_match(rest, tail)
    })
}

/// Glob-match one segment: `*` any run, `?` any single character, else literal.
/// Walks both strings by character boundary in place, so the watcher's
/// per-event check allocates nothing.
pub(crate) fn segment_matches(pattern: &str, name: &str) -> bool {
    let mut pattern_characters = pattern.chars();
    let mut name_characters = name.chars();
    match pattern_characters.next() {
        None => name.is_empty(),
        Some('*') => {
            let rest = pattern_characters.as_str();
            name.char_indices()
                .map(|(offset, _)| offset)
                .chain(std::iter::once(name.len()))
                .any(|offset| segment_matches(rest, &name[offset..]))
        }
        Some('?') => {
            name_characters.next().is_some()
                && segment_matches(pattern_characters.as_str(), name_characters.as_str())
        }
        Some(literal) => {
            name_characters.next() == Some(literal)
                && segment_matches(pattern_characters.as_str(), name_characters.as_str())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{segment_matches, segments_match};

    #[test]
    fn a_star_stays_within_one_segment() {
        assert!(segments_match(&["packages", "*"], &["packages", "app"]));
        assert!(!segments_match(
            &["packages", "*"],
            &["packages", "app", "src"]
        ));
    }

    #[test]
    fn a_double_star_spans_any_depth_including_none() {
        assert!(segments_match(&["**", "tmp"], &["tmp"]));
        assert!(segments_match(
            &["apps", "**", "tmp"],
            &["apps", "web", "cache", "tmp"]
        ));
        assert!(!segments_match(
            &["apps", "**", "tmp"],
            &["apps", "tmp.txt"]
        ));
    }

    #[test]
    fn a_question_mark_takes_exactly_one_whole_character() {
        assert!(segment_matches("t?st", "tést"));
        assert!(!segment_matches("t?st", "tst"));
    }
}
