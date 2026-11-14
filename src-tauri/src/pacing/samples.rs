//! Each account window's recent working readings, for its burn rate - the
//! engine's copy of the TypeScript's `samples` map and its `localStorage`
//! mirror. An account is shared by every agent billing it, so their work adds
//! up in one history per window; the book is kept on disk so a restart doesn't
//! throw away the ten minutes of work a burn rate takes to measure.
//!
//! The book is plain data: the clock is a parameter and the file I/O is two
//! functions the wiring calls from a blocking task.

use std::collections::BTreeMap;
use std::io;
use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::pacing::facts::AccountKey;
use crate::pacing::units::{BurnRate, EpochMilliseconds};
use crate::pacing::usage_pace::{burn_rate, UsageSample, RATE_HISTORY_MS};
use crate::usage::UsageWindow;

/// Where the book lives under the PADE data directory.
pub const SAMPLES_FILE_NAME: &str = "pacing-samples.json";

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct SampleBook {
    /// Account, then window key, to its readings oldest first.
    histories: BTreeMap<AccountKey, BTreeMap<String, Vec<UsageSample>>>,
}

#[derive(Debug, Clone, Copy)]
pub struct RecordParams<'a> {
    pub account_key: &'a AccountKey,
    pub windows: &'a [UsageWindow],
    pub now: EpochMilliseconds,
}

impl SampleBook {
    /// Take one reading of every window of an account, forgetting the readings
    /// older than the burn rate's history.
    pub fn record(&mut self, params: RecordParams<'_>) {
        let RecordParams {
            account_key,
            windows,
            now,
        } = params;
        let account = self.histories.entry(account_key.clone()).or_default();
        for window in windows {
            let history = account.entry(window.key.clone()).or_default();
            history.retain(|sample| sample.time >= now - RATE_HISTORY_MS);
            history.push(UsageSample {
                time: now,
                utilization: crate::pacing::usage_pace::utilization_of(window),
                resets_at: window.resets_at.clone(),
            });
        }
    }

    /// Start an account's burn rate over - after a step, the old pace no longer
    /// says anything about the new one.
    pub fn forget_account(&mut self, account_key: &AccountKey) {
        self.histories.remove(account_key);
    }

    /// How fast an account's window is being spent while its agents work, or
    /// `None` until enough working time is behind it.
    #[must_use]
    pub fn rate_of(&self, account_key: &AccountKey, window: &UsageWindow) -> Option<BurnRate> {
        let history = self.histories.get(account_key)?.get(&window.key)?;
        burn_rate(history)
    }

    /// The book at `path`; an empty one while there is none or it can't be read
    /// (a burn rate measured afresh is the honest fallback).
    #[must_use]
    pub fn load(path: &Path) -> Self {
        std::fs::read_to_string(path)
            .ok()
            .and_then(|raw| serde_json::from_str(&raw).ok())
            .unwrap_or_default()
    }

    /// Write the book to `path` whole, through a sibling file so a crash never
    /// leaves half of it.
    pub fn save(&self, path: &Path) -> io::Result<()> {
        let raw = serde_json::to_string(self).map_err(io::Error::other)?;
        let sibling = path.with_extension("tmp");
        std::fs::write(&sibling, raw)?;
        std::fs::rename(&sibling, path)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pacing::test_support::{account, session_window, NOW};
    use crate::pacing::units::SpanMilliseconds;

    fn key(name: &str) -> AccountKey {
        AccountKey::new(name)
    }

    /// A book with readings every minute, the window rising `rise` per minute.
    fn worked_book(account_key: &AccountKey, minutes: u32, rise: f64) -> (SampleBook, UsageWindow) {
        let mut book = SampleBook::default();
        let mut last = session_window(0.0, SpanMilliseconds::from_hours(3.0));
        for minute in 0..=minutes {
            let used = f64::from(minute) * rise;
            last = session_window(used, SpanMilliseconds::from_hours(3.0));
            book.record(RecordParams {
                account_key,
                windows: std::slice::from_ref(&last),
                now: NOW + SpanMilliseconds::from_minutes(f64::from(minute)),
            });
        }
        (book, last)
    }

    #[test]
    fn a_burn_rate_needs_ten_minutes_of_working_readings() {
        let account_key = key("anthropic");
        let (short, window) = worked_book(&account_key, 5, 1.0);
        assert_eq!(short.rate_of(&account_key, &window), None);
        let (long, window) = worked_book(&account_key, 12, 1.0);
        let rate = long
            .rate_of(&account_key, &window)
            .expect("a measured rate");
        let per_minute = rate.get() * 60_000.0;
        assert!((per_minute - 1.0).abs() < 1e-9, "got {per_minute}");
    }

    #[test]
    fn readings_older_than_the_history_are_forgotten() {
        let account_key = key("anthropic");
        let (mut book, window) = worked_book(&account_key, 10, 1.0);
        let later = NOW + SpanMilliseconds::from_hours(2.0);
        book.record(RecordParams {
            account_key: &account_key,
            windows: std::slice::from_ref(&window),
            now: later,
        });
        let history = &book.histories[&account_key][&window.key];
        assert_eq!(history.len(), 1);
        assert_eq!(history[0].time, later);
    }

    #[test]
    fn accounts_keep_separate_histories() {
        let anthropic = key("anthropic");
        let (mut book, window) = worked_book(&anthropic, 12, 1.0);
        let other = key("chatgpt");
        assert_eq!(book.rate_of(&other, &window), None);
        book.forget_account(&anthropic);
        assert_eq!(book.rate_of(&anthropic, &window), None);
    }

    #[test]
    fn recording_an_account_reads_every_window() {
        let account_key = key("anthropic");
        let windows = account(vec![
            session_window(10.0, SpanMilliseconds::from_hours(3.0)),
            crate::pacing::test_support::weekly_window(20.0, SpanMilliseconds::from_days(3.0)),
        ])
        .windows;
        let mut book = SampleBook::default();
        book.record(RecordParams {
            account_key: &account_key,
            windows: &windows,
            now: NOW,
        });
        assert_eq!(book.histories[&account_key].len(), 2);
    }

    #[test]
    fn the_book_survives_a_round_trip_through_a_file() {
        let account_key = key("anthropic");
        let (book, window) = worked_book(&account_key, 12, 1.0);
        let directory = std::env::temp_dir().join(format!("pade-samples-{}", std::process::id()));
        std::fs::create_dir_all(&directory).expect("a scratch directory");
        let path = directory.join(SAMPLES_FILE_NAME);

        book.save(&path).expect("saves");
        let loaded = SampleBook::load(&path);
        std::fs::remove_dir_all(&directory).expect("cleans up");

        assert_eq!(loaded, book);
        assert_eq!(
            loaded.rate_of(&account_key, &window),
            book.rate_of(&account_key, &window)
        );
    }

    #[test]
    fn a_missing_or_broken_file_is_an_empty_book() {
        let directory =
            std::env::temp_dir().join(format!("pade-samples-bad-{}", std::process::id()));
        std::fs::create_dir_all(&directory).expect("a scratch directory");
        let path = directory.join(SAMPLES_FILE_NAME);
        assert_eq!(SampleBook::load(&path), SampleBook::default());
        std::fs::write(&path, "not json").expect("writes");
        assert_eq!(SampleBook::load(&path), SampleBook::default());
        std::fs::remove_dir_all(&directory).expect("cleans up");
    }
}
