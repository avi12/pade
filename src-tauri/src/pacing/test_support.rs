//! What the pacing tests share: window builders on a fixed clock, and the
//! tolerance every golden-fixture comparison uses.

use serde_json::Value;

use crate::pacing::units::{EpochMilliseconds, SpanMilliseconds};
use crate::usage::{AccountUsage, UsageWindow, UsageWindowKind};

/// 2026-10-15T12:00:00Z, the clock the ported tests run on.
pub const NOW: EpochMilliseconds = EpochMilliseconds::new(1_792_065_600_000.0);

/// How far a Rust float may sit from the TypeScript's, relative to its size
/// (and absolutely below 1). The two run the same IEEE operations in the same
/// order, so they agree to the last bit in practice; the tolerance only keeps a
/// harmless reassociation from failing the suite.
const RELATIVE_TOLERANCE: f64 = 1e-9;

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

/// Asserts two JSON values are the same shape and agree number by number within
/// `RELATIVE_TOLERANCE`; `context` names the fixture case in a failure.
pub fn assert_json_close(actual: &Value, expected: &Value, context: &str) {
    match (actual, expected) {
        (Value::Number(actual), Value::Number(expected)) => {
            let (actual, expected) = (
                actual.as_f64().expect("a finite number"),
                expected.as_f64().expect("a finite number"),
            );
            let allowed = RELATIVE_TOLERANCE * expected.abs().max(1.0);
            assert!(
                (actual - expected).abs() <= allowed,
                "{context}: {actual} is not {expected}"
            );
        }
        (Value::Array(actual), Value::Array(expected)) => {
            assert_eq!(actual.len(), expected.len(), "{context}: array length");
            for (index, (actual, expected)) in actual.iter().zip(expected).enumerate() {
                assert_json_close(actual, expected, &format!("{context}[{index}]"));
            }
        }
        (Value::Object(actual), Value::Object(expected)) => {
            let keys = |object: &serde_json::Map<String, Value>| {
                let mut keys: Vec<String> = object.keys().cloned().collect();
                keys.sort();
                keys
            };
            assert_eq!(keys(actual), keys(expected), "{context}: object keys");
            for (key, expected) in expected {
                assert_json_close(&actual[key], expected, &format!("{context}.{key}"));
            }
        }
        _ => assert_eq!(actual, expected, "{context}"),
    }
}
