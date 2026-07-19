//! A small `.gitignore` matcher for workspaces git can't answer for.
//!
//! The Change Feed's git-mode ignore policy defers to `git check-ignore`, which
//! is authoritative. But a workspace with no repo yet — a temp workspace the
//! create flow just scaffolded, a folder before its `git init` — can still hold
//! a `.gitignore` describing exactly what its tools will generate, and the feed
//! should honor it there too. This module parses the ROOT `.gitignore` only
//! (nested files are a git-mode concern; git handles them itself) and matches
//! the documented core of the syntax: comments and blanks, `!` negation with
//! last-match-wins, trailing-`/` directory-only patterns, leading-or-inner-`/`
//! anchoring, `*`/`?` within a segment, and `**` across segments. A parent
//! directory that is ignored ignores everything below it — matching git, where
//! a file can't be re-included under an excluded directory.

/// One parsed `.gitignore` pattern.
struct Pattern {
    /// The `/`-split glob segments (anchored) or the bare-name glob (floating).
    segments: Vec<String>,
    /// `!`-prefixed: a match re-includes instead of ignoring.
    negated: bool,
    /// Trailing `/`: the pattern only matches directories.
    directory_only: bool,
    /// Held a `/` (leading or inner): matched against the whole root-relative
    /// path. A bare name floats: it matches its basename at any depth.
    anchored: bool,
}

/// The parsed rules of one `.gitignore`, ready to answer [`Rules::is_ignored`].
/// An empty file (or none at all) parses to rules that ignore nothing.
#[derive(Default)]
pub struct Rules {
    patterns: Vec<Pattern>,
}

impl Rules {
    /// Parse `.gitignore` content. Unsupported edges (escapes, trailing-space
    /// significance) are simply treated literally — this backs a change feed,
    /// not git itself.
    pub fn parse(content: &str) -> Self {
        let mut patterns = Vec::new();
        for line in content.lines() {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            let (negated, body) = match line.strip_prefix('!') {
                Some(rest) => (true, rest),
                None => (false, line),
            };
            let (directory_only, body) = match body.strip_suffix('/') {
                Some(rest) => (true, rest),
                None => (false, body),
            };
            let anchored = body.contains('/');
            let body = body.strip_prefix('/').unwrap_or(body);
            if body.is_empty() {
                continue;
            }
            patterns.push(Pattern {
                segments: body.split('/').map(str::to_string).collect(),
                negated,
                directory_only,
                anchored,
            });
        }
        Self { patterns }
    }

    /// Whether the rules ignore the FILE at the root-relative, `/`-joined
    /// `path`. Checks every ancestor directory first: an ignored ancestor
    /// ignores the file outright (git allows no re-include below an excluded
    /// directory); otherwise the file's own last matching pattern decides.
    pub fn is_ignored(&self, path: &str) -> bool {
        let segments: Vec<&str> = path.split('/').filter(|s| !s.is_empty()).collect();
        if segments.is_empty() {
            return false;
        }
        for prefix_length in 1..=segments.len() {
            let target = &segments[..prefix_length];
            let target_is_directory = prefix_length < segments.len();
            if self.last_match(target, target_is_directory) == Some(Verdict::Ignored) {
                if target_is_directory {
                    return true;
                }
                return true;
            } else if prefix_length == segments.len() {
                return false;
            }
        }
        false
    }

    /// The verdict of the last pattern matching `target`, or `None` when no
    /// pattern matches — last-match-wins is gitignore's precedence rule.
    fn last_match(&self, target: &[&str], target_is_directory: bool) -> Option<Verdict> {
        let mut verdict = None;
        for pattern in &self.patterns {
            if pattern.directory_only && !target_is_directory {
                continue;
            }
            if pattern.matches(target) {
                verdict = Some(if pattern.negated {
                    Verdict::Included
                } else {
                    Verdict::Ignored
                });
            }
        }
        verdict
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Verdict {
    Ignored,
    Included,
}

impl Pattern {
    fn matches(&self, target: &[&str]) -> bool {
        if self.anchored {
            return segments_match(&self.segments, target);
        }
        // A floating pattern matches the basename at any depth.
        target
            .last()
            .is_some_and(|name| segment_matches(&self.segments[0], name))
    }
}

/// Whether glob `pattern` segments match `path` segments exactly; `**` spans
/// any number of them.
fn segments_match(pattern: &[String], path: &[&str]) -> bool {
    match pattern.split_first() {
        None => path.is_empty(),
        Some((head, rest)) if head == "**" => {
            (0..=path.len()).any(|skip| segments_match(rest, &path[skip..]))
        }
        Some((head, rest)) => {
            let Some((first, tail)) = path.split_first() else {
                return false;
            };
            segment_matches(head, first) && segments_match(rest, tail)
        }
    }
}

/// Glob-match one segment: `*` any run, `?` any single character, else literal.
fn segment_matches(pattern: &str, name: &str) -> bool {
    let pattern: Vec<char> = pattern.chars().collect();
    let name: Vec<char> = name.chars().collect();
    chars_match(&pattern, &name)
}

fn chars_match(pattern: &[char], name: &[char]) -> bool {
    match pattern.split_first() {
        None => name.is_empty(),
        Some(('*', rest)) => (0..=name.len()).any(|skip| chars_match(rest, &name[skip..])),
        Some(('?', rest)) => name
            .split_first()
            .is_some_and(|(_, tail)| chars_match(rest, tail)),
        Some((literal, rest)) => name
            .split_first()
            .is_some_and(|(first, tail)| first == literal && chars_match(rest, tail)),
    }
}

#[cfg(test)]
mod tests {
    use super::Rules;

    #[test]
    fn bare_names_match_at_any_depth() {
        let rules = Rules::parse("dist\n*.log\n");
        assert!(rules.is_ignored("dist"));
        assert!(rules.is_ignored("packages/app/dist"));
        assert!(rules.is_ignored("deep/nested/debug.log"));
        assert!(!rules.is_ignored("src/main.ts"));
    }

    #[test]
    fn an_ignored_directory_ignores_everything_below_it() {
        let rules = Rules::parse("build/\n");
        assert!(rules.is_ignored("build/output.js"));
        assert!(rules.is_ignored("tools/build/deep/file.rs"));
        // Directory-only: a FILE named build is not matched.
        assert!(!rules.is_ignored("build"));
    }

    #[test]
    fn a_slash_anchors_to_the_root() {
        let rules = Rules::parse("/coverage\ndocs/generated\n");
        assert!(rules.is_ignored("coverage"));
        assert!(!rules.is_ignored("packages/app/coverage"));
        assert!(rules.is_ignored("docs/generated/api.md"));
        assert!(!rules.is_ignored("other/docs/generated"));
    }

    #[test]
    fn negation_wins_as_the_last_match() {
        let rules = Rules::parse("*.env\n!example.env\n");
        assert!(rules.is_ignored("secrets.env"));
        assert!(!rules.is_ignored("example.env"));
        // Order matters: a later ignore beats an earlier re-include.
        let reversed = Rules::parse("!example.env\n*.env\n");
        assert!(reversed.is_ignored("example.env"));
    }

    #[test]
    fn no_reinclude_below_an_excluded_directory() {
        let rules = Rules::parse("generated/\n!generated/keep.md\n");
        // Git's own rule: the excluded parent wins over the child re-include.
        assert!(rules.is_ignored("generated/keep.md"));
    }

    #[test]
    fn double_star_spans_directories() {
        let rules = Rules::parse("**/fixtures/*.snap\napps/**/tmp\n");
        assert!(rules.is_ignored("tests/fixtures/a.snap"));
        assert!(rules.is_ignored("fixtures/b.snap"));
        assert!(rules.is_ignored("apps/web/cache/tmp"));
        assert!(!rules.is_ignored("apps/tmp.txt"));
    }

    #[test]
    fn comments_and_blanks_ignore_nothing() {
        let rules = Rules::parse("# a comment\n\n   \n");
        assert!(!rules.is_ignored("anything.txt"));
    }
}
