//! What the pacing tests share: window builders on a fixed clock.

use crate::pacing::units::{EpochMilliseconds, SpanMilliseconds};
use crate::usage::{AccountUsage, UsageWindow, UsageWindowKind};

/// 2026-10-15T12:00:00Z, the clock the ported tests run on.
pub const NOW: EpochMilliseconds = EpochMilliseconds::new(1_792_065_600_000.0);

/// `time` as the ISO string the usage endpoint would send.
pub fn iso(time: EpochMilliseconds) -> String {
    let seconds: i64 = (time.get() / 1_000.0)
        .round()
        .to_string()
        .parse()
        .expect("a whole number of seconds");
    crate::usage::unix_seconds_to_iso8601(seconds)
}

fn window(
    kind: UsageWindowKind,
    key: &str,
    utilization: f64,
    resets_in: SpanMilliseconds,
) -> UsageWindow {
    UsageWindow {
        key: key.to_string(),
        kind,
        label: key.to_string(),
        utilization,
        resets_at: Some(iso(NOW + resets_in)),
    }
}

pub fn session_window(utilization: f64, resets_in: SpanMilliseconds) -> UsageWindow {
    window(
        UsageWindowKind::Session,
        "five_hour",
        utilization,
        resets_in,
    )
}

pub fn weekly_window(utilization: f64, resets_in: SpanMilliseconds) -> UsageWindow {
    window(UsageWindowKind::Weekly, "seven_day", utilization, resets_in)
}

pub fn account(windows: Vec<UsageWindow>) -> AccountUsage {
    let mut account = AccountUsage::default();
    account.windows.extend(windows);
    account
}
