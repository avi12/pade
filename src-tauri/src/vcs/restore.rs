//! Restore a version — natural-language query → ranked commits → checkout.

use std::time::{SystemTime, UNIX_EPOCH};

use serde::Serialize;

use super::{run_git, US};

/// A commit ranked against a natural-language restore query.
/// Mirrors `Commit`'s wire shape (camelCase) plus a relevance `score`.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RestoreCandidate {
    id: String,
    short: String,
    summary: String,
    author: String,
    when: String,
    score: f32,
}

const DEFAULT_CANDIDATE_LIMIT: u32 = 50;
/// Boost added to a commit whose committer time falls inside a query's time hint.
const TIME_HINT_BOOST: f32 = 0.5;

/// Seconds in a day — the unit for relative time-hint windows.
const DAY_SECS: u64 = 86_400;

/// Current unix time in seconds (0 if the clock predates the epoch).
fn now_unix() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

/// Split a string into lowercase alphanumeric-ish tokens (whitespace-separated).
fn tokenize(text: &str) -> Vec<String> {
    text.split_whitespace()
        .map(str::to_lowercase)
        .filter(|t| !t.is_empty())
        .collect()
}

/// An inclusive `[start, end]` unix-time window a commit's `%ct` can fall into.
struct TimeWindow {
    start: u64,
    end: u64,
}

impl TimeWindow {
    fn contains(&self, ts: u64) -> bool {
        ts >= self.start && ts <= self.end
    }
}

/// Derive a unix-time window from natural-language time hints in `query`, if any.
/// A small, dependency-free hint table; `now` is the reference "now".
fn time_hint_window(query: &str, now: u64) -> Option<TimeWindow> {
    let q = query.to_lowercase();

    // "N days ago" — window is that whole calendar-ish day (± around the offset).
    if let Some(days) = parse_days_ago(&q) {
        let offset = days.saturating_mul(DAY_SECS);
        let center = now.saturating_sub(offset);
        return Some(TimeWindow {
            start: center.saturating_sub(DAY_SECS),
            end: center,
        });
    }

    let contains_today = q.contains("today");
    if contains_today {
        return Some(TimeWindow {
            start: now.saturating_sub(DAY_SECS),
            end: now,
        });
    }

    let contains_yesterday = q.contains("yesterday");
    if contains_yesterday {
        return Some(TimeWindow {
            start: now.saturating_sub(2 * DAY_SECS),
            end: now.saturating_sub(DAY_SECS),
        });
    }

    let contains_last_week = q.contains("last week");
    if contains_last_week {
        return Some(TimeWindow {
            start: now.saturating_sub(7 * DAY_SECS),
            end: now,
        });
    }

    None
}

/// Parse a leading "N days ago" pattern out of an already-lowercased query.
/// Returns the number of days, or None when the phrase isn't present.
fn parse_days_ago(query: &str) -> Option<u64> {
    let idx = query.find("days ago").or_else(|| query.find("day ago"))?;
    // Walk back over the digits immediately preceding the phrase.
    let head = query[..idx].trim_end();
    let digits: String = head
        .chars()
        .rev()
        .take_while(char::is_ascii_digit)
        .collect::<String>()
        .chars()
        .rev()
        .collect();
    digits.parse::<u64>().ok()
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
    let n = limit.unwrap_or(DEFAULT_CANDIDATE_LIMIT);
    // %ct is the committer unix timestamp, used for time-hint scoring.
    let fmt = format!("%H{US}%h{US}%s{US}%an{US}%cr{US}%ct");
    let raw = run_git(
        &cwd,
        &["log", &format!("-n{n}"), &format!("--pretty=format:{fmt}")],
    )?;

    let query_tokens = tokenize(&query);
    let now = now_unix();
    let window = time_hint_window(&query, now);

    let mut candidates: Vec<RestoreCandidate> = raw
        .lines()
        .filter_map(|line| {
            let f: Vec<&str> = line.split(US).collect();
            let [id, short, summary, author, when, ct] = f.as_slice() else {
                return None;
            };

            let subject = summary.to_lowercase();
            let matched = query_tokens
                .iter()
                .filter(|tok| subject.contains(tok.as_str()))
                .count();
            // Token counts are tiny (a short query vs a commit subject), so the
            // usize→f32 conversion can't actually lose precision here.
            #[allow(clippy::cast_precision_loss)]
            let mut score = if query_tokens.is_empty() {
                0.0
            } else {
                matched as f32 / query_tokens.len() as f32
            };

            // Time-hint boost: reward commits committed inside the hinted window.
            let in_time_window = window
                .as_ref()
                .zip(ct.parse::<u64>().ok())
                .is_some_and(|(win, ts)| win.contains(ts));
            if in_time_window {
                score += TIME_HINT_BOOST;
            }

            Some(RestoreCandidate {
                id: (*id).into(),
                short: (*short).into(),
                summary: (*summary).into(),
                author: (*author).into(),
                when: (*when).into(),
                score,
            })
        })
        .collect();

    // Stable sort by score desc; equal scores keep git-log (recency) order.
    candidates.sort_by(|a, b| {
        b.score
            .partial_cmp(&a.score)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    Ok(candidates)
}

/// Restore a commit non-destructively by switching to a dedicated branch.
///
/// Creates (or re-enters) `pade/restore-<shortsha>` pointing at `sha` — never
/// touches the working tree with `reset --hard`. Returns the branch name.
#[tauri::command]
pub async fn vcs_restore_checkout(cwd: String, sha: String) -> Result<String, String> {
    let short = run_git(&cwd, &["rev-parse", "--short", &sha])?
        .trim()
        .to_string();
    let branch = format!("pade/restore-{short}");

    // Does the restore branch already exist? Then just switch to it.
    let branch_exists = run_git(&cwd, &["rev-parse", "--verify", "--quiet", &branch]).is_ok();
    if branch_exists {
        run_git(&cwd, &["switch", &branch])?;
    } else {
        // Create the branch at the target commit and switch to it in one step.
        // Any error (e.g. a dirty working tree) surfaces git's stderr verbatim.
        run_git(&cwd, &["switch", "-c", &branch, &sha])?;
    }
    Ok(branch)
}

#[cfg(test)]
mod tests {
    use super::{parse_days_ago, time_hint_window, tokenize, DAY_SECS};

    const NOW: u64 = 100 * DAY_SECS;

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
        assert!(window.contains(NOW - DAY_SECS));
        assert!(!window.contains(NOW - 2 * DAY_SECS));
    }

    #[test]
    fn yesterday_excludes_the_current_day() {
        let window = time_hint_window("yesterday's version", NOW).expect("window");
        assert!(window.contains(NOW - DAY_SECS - 1));
        assert!(!window.contains(NOW));
    }

    #[test]
    fn n_days_ago_centers_on_that_day() {
        let window = time_hint_window("5 days ago", NOW).expect("window");
        assert!(window.contains(NOW - 5 * DAY_SECS));
        assert!(window.contains(NOW - 5 * DAY_SECS - DAY_SECS / 2));
        assert!(!window.contains(NOW));
    }

    #[test]
    fn last_week_spans_seven_days() {
        let window = time_hint_window("last week", NOW).expect("window");
        assert!(window.contains(NOW));
        assert!(window.contains(NOW - 7 * DAY_SECS));
        assert!(!window.contains(NOW - 8 * DAY_SECS));
    }

    #[test]
    fn plain_text_has_no_window() {
        assert!(time_hint_window("the meter change", NOW).is_none());
    }
}
