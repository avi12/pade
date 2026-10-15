//! Restore a version - natural-language query → ranked commits → checkout.

use serde::Serialize;

use super::{commit_header_format, run_git, validate_object_id, CommitHeader, FIELD_SEPARATOR};
use crate::util::since_unix_epoch;

/// A commit ranked against a natural-language restore query: the Log view's
/// header fields plus a relevance `score`.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RestoreCandidate {
    #[serde(flatten)]
    header: CommitHeader,
    score: f32,
}

const DEFAULT_CANDIDATE_LIMIT: u32 = 50;
/// Boost added to a commit whose committer time falls inside a query's time hint.
const TIME_HINT_BOOST: f32 = 0.5;

/// Seconds in a day - the unit for relative time-hint windows.
const DAY_SECONDS: u64 = 86_400;
/// How far back "last week" reaches.
const WEEK_DAYS: u64 = 7;

/// The branch-name prefix a restored commit is checked out under.
const RESTORE_BRANCH_PREFIX: &str = "pade/restore-";

/// Split a string into lowercase alphanumeric-ish tokens (whitespace-separated).
fn tokenize(text: &str) -> Vec<String> {
    text.split_whitespace()
        .map(str::to_lowercase)
        .filter(|token| !token.is_empty())
        .collect()
}

/// An inclusive `[start, end]` unix-time window a commit's `%ct` can fall into.
struct TimeWindow {
    start: u64,
    end: u64,
}

impl TimeWindow {
    fn contains(&self, timestamp: u64) -> bool {
        timestamp >= self.start && timestamp <= self.end
    }
}

/// Derive a unix-time window from natural-language time hints in `query`, if any.
/// A small, dependency-free hint table; `now` is the reference "now".
fn time_hint_window(query: &str, now: u64) -> Option<TimeWindow> {
    let normalized_query = query.to_lowercase();

    // "N days ago" - window is that whole calendar-ish day (± around the offset).
    if let Some(days) = parse_days_ago(&normalized_query) {
        let offset = days.saturating_mul(DAY_SECONDS);
        let center = now.saturating_sub(offset);
        return Some(TimeWindow {
            start: center.saturating_sub(DAY_SECONDS),
            end: center,
        });
    }

    let contains_today = normalized_query.contains("today");
    if contains_today {
        return Some(TimeWindow {
            start: now.saturating_sub(DAY_SECONDS),
            end: now,
        });
    }

    let contains_yesterday = normalized_query.contains("yesterday");
    if contains_yesterday {
        let one_day_ago = now.saturating_sub(DAY_SECONDS);
        return Some(TimeWindow {
            start: one_day_ago.saturating_sub(DAY_SECONDS),
            end: one_day_ago,
        });
    }

    let contains_last_week = normalized_query.contains("last week");
    if contains_last_week {
        return Some(TimeWindow {
            start: now.saturating_sub(WEEK_DAYS * DAY_SECONDS),
            end: now,
        });
    }

    None
}

/// Parse a leading "N days ago" pattern out of an already-lowercased query.
/// Returns the number of days, or None when the phrase isn't present.
fn parse_days_ago(query: &str) -> Option<u64> {
    let index = query.find("days ago").or_else(|| query.find("day ago"))?;
    let head = query[..index].trim_end();
    // ASCII digits are one byte each, so the digit count is also their byte length.
    let digit_count = head.chars().rev().take_while(char::is_ascii_digit).count();
    head[head.len() - digit_count..].parse::<u64>().ok()
}

/// Rank recent commits against a natural-language `query`.
///
/// Scores each commit by the fraction of query tokens that appear as substrings
/// of its subject, plus a time-hint boost (e.g. "yesterday", "3 days ago").
/// Returns all candidates, best score first, ties keeping git-log recency order.
#[tauri::command]
pub async fn vcs_restore_candidates(
    cwd: String,
    query: String,
    limit: Option<u32>,
) -> Result<Vec<RestoreCandidate>, String> {
    let candidate_limit = limit.unwrap_or(DEFAULT_CANDIDATE_LIMIT);
    // %ct is the committer unix timestamp, used for time-hint scoring.
    let format = format!(
        "--pretty=format:{}{FIELD_SEPARATOR}%ct",
        commit_header_format()
    );
    let raw = run_git(&cwd, &["log", &format!("-n{candidate_limit}"), &format])?;

    let query_tokens = tokenize(&query);
    let window = time_hint_window(&query, since_unix_epoch().as_secs());

    let mut candidates: Vec<RestoreCandidate> = raw
        .lines()
        .filter_map(|line| {
            let fields: Vec<&str> = line.split(FIELD_SEPARATOR).collect();
            let (header, [committed_at]) = CommitHeader::split_off(&fields)? else {
                return None;
            };
            let is_in_time_window = window
                .as_ref()
                .zip(committed_at.parse::<u64>().ok())
                .is_some_and(|(window, timestamp)| window.contains(timestamp));
            let time_boost = if is_in_time_window {
                TIME_HINT_BOOST
            } else {
                0.0
            };
            let score = subject_match_ratio(&header.summary, &query_tokens) + time_boost;
            Some(RestoreCandidate { header, score })
        })
        .collect();

    // Stable sort by score desc; equal scores keep git-log (recency) order.
    candidates.sort_by(|left, right| right.score.total_cmp(&left.score));
    Ok(candidates)
}

/// The fraction of `query_tokens` found as substrings of `subject`, or zero
/// for an empty query.
fn subject_match_ratio(subject: &str, query_tokens: &[String]) -> f32 {
    if query_tokens.is_empty() {
        return 0.0;
    }
    let subject = subject.to_lowercase();
    let matched = query_tokens
        .iter()
        .filter(|token| subject.contains(token.as_str()))
        .count();
    // Token counts are tiny (a short query vs a commit subject), so the
    // usize to f32 conversion can't actually lose precision here.
    #[allow(clippy::cast_precision_loss)]
    let ratio = matched as f32 / query_tokens.len() as f32;
    ratio
}

/// Restore a commit non-destructively by switching to a dedicated branch.
///
/// Creates (or re-enters) `pade/restore-<shortsha>` pointing at `sha` - never
/// touches the working tree with `reset --hard`. Returns the branch name.
#[tauri::command]
pub async fn vcs_restore_checkout(cwd: String, sha: String) -> Result<String, String> {
    let sha = validate_object_id(&sha)?;
    let short = run_git(&cwd, &["rev-parse", "--short", "--end-of-options", sha])?
        .trim()
        .to_string();
    let branch = format!("{RESTORE_BRANCH_PREFIX}{short}");

    // Does the restore branch already exist? Then just switch to it.
    let branch_exists = run_git(&cwd, &["rev-parse", "--verify", "--quiet", &branch]).is_ok();
    if branch_exists {
        run_git(&cwd, &["switch", &branch])?;
    } else {
        // Create the branch at the target commit and switch to it in one step.
        // Any error (e.g. a dirty working tree) surfaces git's stderr verbatim.
        run_git(&cwd, &["switch", "-c", &branch, sha])?;
    }
    Ok(branch)
}

#[cfg(test)]
mod tests {
    use super::{parse_days_ago, time_hint_window, tokenize, DAY_SECONDS};

    const NOW: u64 = 100 * DAY_SECONDS;

    #[test]
    fn tokenize_lowercases_and_splits_on_whitespace() {
        assert_eq!(tokenize("Fix  the Meter"), vec!["fix", "the", "meter"]);
        assert_eq!(tokenize("   "), Vec::<String>::new());
    }

    #[test]
    fn parse_days_ago_reads_the_preceding_number() {
        assert_eq!(parse_days_ago("before 3 days ago"), Some(3));
        assert_eq!(parse_days_ago("1 day ago"), Some(1));
        assert_eq!(parse_days_ago("some days ago"), None);
        assert_eq!(parse_days_ago("no hint here"), None);
    }

    #[test]
    fn today_covers_the_last_day() {
        let window = time_hint_window("what I did today", NOW).expect("window");
        assert!(window.contains(NOW));
        assert!(window.contains(NOW - DAY_SECONDS));
        assert!(!window.contains(NOW - 2 * DAY_SECONDS));
    }

    #[test]
    fn yesterday_excludes_the_current_day() {
        let window = time_hint_window("yesterday's version", NOW).expect("window");
        assert!(window.contains(NOW - DAY_SECONDS - 1));
        assert!(!window.contains(NOW));
    }

    #[test]
    fn n_days_ago_centers_on_that_day() {
        let window = time_hint_window("5 days ago", NOW).expect("window");
        assert!(window.contains(NOW - 5 * DAY_SECONDS));
        assert!(window.contains(NOW - 5 * DAY_SECONDS - DAY_SECONDS / 2));
        assert!(!window.contains(NOW));
    }

    #[test]
    fn last_week_spans_seven_days() {
        let window = time_hint_window("last week", NOW).expect("window");
        assert!(window.contains(NOW));
        assert!(window.contains(NOW - 7 * DAY_SECONDS));
        assert!(!window.contains(NOW - 8 * DAY_SECONDS));
    }

    #[test]
    fn plain_text_has_no_window() {
        assert!(time_hint_window("the meter change", NOW).is_none());
    }
}
