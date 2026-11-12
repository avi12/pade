//! Muse Code usage adapter: the allowance `muse usage --json` prints.

use super::plan_label;
use super::readings::{AccountUsage, BillingAccount, NoReadingReason, UsageWindow};
use super::sign_in::parse_usage_body;
use super::window_time::{
    humanize_window_seconds, unix_seconds_to_iso8601, window_kind_for_duration,
    FIVE_HOUR_WINDOW_SECONDS, SECONDS_PER_MINUTE, WEEKLY_WINDOW_SECONDS,
};
use crate::agents::ID_MUSE;
use std::time::Duration;

// ---------------------------------------------------------------------------
// Muse Code usage. Muse Code 1.4.13 added `muse usage --json`, which prints the
// subscription allowance - the 5-hour window, the rolling weekly window and the
// plan tier - without running a model turn (changelog:
// https://dev.meta.ai/docs/muse-code/changelog; plans:
// https://dev.meta.ai/docs/muse-code/subscriptions). The CLI itself refreshes
// it at most once a minute, and wherever it keeps the login (a file or the OS
// keychain) is its own business. We deliberately do not call the endpoint
// underneath (`api.meta.ai/muse-code/key`): every call there mints a fresh API
// key.
// ---------------------------------------------------------------------------

/// The subcommand that prints Muse's allowance as one JSON object.
const MUSE_USAGE_ARGUMENTS: [&str; 2] = ["usage", "--json"];
/// How long `muse usage` may take - one network refresh at most - before PADE
/// gives up on it and kills it.
const MUSE_USAGE_TIMEOUT: Duration = Duration::from_secs(15);
const MUSE_PLAN: &str = "Muse";
const MUSE_USAGE_SOURCE: &str = "cli:muse usage";
/// The allowance's two windows: the 5-hour block and the rolling week.
const MUSE_SESSION_KEY: &str = "window";
const MUSE_WEEKLY_KEY: &str = "weekly";
const MILLISECONDS_PER_SECOND: i64 = 1_000;

/// What `muse usage` prints when it can't answer, and what each means. Matched
/// case-insensitively against whatever it printed.
const MUSE_FAILURES: [(&str, NoReadingReason); 5] = [
    ("not logged in", NoReadingReason::SignedOut),
    ("no meta-account login", NoReadingReason::SignedOut),
    ("no longer valid", NoReadingReason::SignInExpired),
    ("credential_rejected", NoReadingReason::SignInExpired),
    // Pay-as-you-go, or no subscription: there is no allowance to read.
    ("no subscription allowance", NoReadingReason::NoSource),
];

pub(super) fn fetch_muse_account_usage() -> Result<AccountUsage, NoReadingReason> {
    let executable = crate::agents::program(ID_MUSE).ok_or(NoReadingReason::NoSource)?;
    let mut command = crate::util::command(executable);
    command.args(MUSE_USAGE_ARGUMENTS);
    let run = crate::util::capture_within(command, MUSE_USAGE_TIMEOUT)
        .ok_or(NoReadingReason::Unreachable)?;
    if !run.is_success {
        return Err(muse_failure(&format!("{}\n{}", run.stdout, run.stderr)));
    }
    muse_account_usage(&parse_usage_body(&run.stdout)?)
}

/// Why `muse usage` failed, from what it printed. Anything unrecognized (a
/// refresh that was rate-limited or failed, no usage data yet in a new window)
/// is transient.
pub(super) fn muse_failure(output: &str) -> NoReadingReason {
    let output = output.to_lowercase();
    MUSE_FAILURES
        .iter()
        .find(|(marker, _)| output.contains(marker))
        .map_or(NoReadingReason::Unreachable, |(_, reason)| *reason)
}

/// The account a `muse usage --json` object describes: `allowance.window` (the
/// 5-hour block) and `allowance.weekly`, each `{ usedPercent, resetsAtMs }`,
/// under the plan's `tier`. An object with no allowance belongs to an account
/// without a subscription - no usage source.
pub(super) fn muse_account_usage(
    response: &serde_json::Value,
) -> Result<AccountUsage, NoReadingReason> {
    let allowance = response
        .get("allowance")
        .filter(|allowance| allowance.is_object())
        .ok_or(NoReadingReason::NoSource)?;
    let windows: Vec<UsageWindow> = [MUSE_SESSION_KEY, MUSE_WEEKLY_KEY]
        .into_iter()
        .filter_map(|key| muse_window(key, allowance.get(key)?))
        .collect();
    let tier = allowance
        .get("tier")
        .and_then(serde_json::Value::as_str)
        .filter(|tier| !tier.is_empty());
    if windows.is_empty() && tier.is_none() {
        return Err(NoReadingReason::Unreachable);
    }

    Ok(AccountUsage {
        windows,
        plan: plan_label(MUSE_PLAN, tier),
        source: MUSE_USAGE_SOURCE.to_string(),
        account: Some(BillingAccount::Meta),
        ..AccountUsage::default()
    })
}

/// One Muse allowance window, classified by its length: `window` is the
/// documented 5-hour block and `weekly` the rolling week, unless the window
/// states another length (`windowDurationMins`, which `muse serve` reports).
/// `usedPercent` is the server's own figure and may pass 100.
fn muse_window(key: &str, value: &serde_json::Value) -> Option<UsageWindow> {
    let utilization = value.get("usedPercent")?.as_f64()?;
    let resets_at = value
        .get("resetsAtMs")
        .and_then(serde_json::Value::as_i64)
        .map(|milliseconds| unix_seconds_to_iso8601(milliseconds / MILLISECONDS_PER_SECOND));
    let minutes = value
        .get("windowDurationMins")
        .and_then(serde_json::Value::as_i64);
    let documented_seconds = if key == MUSE_WEEKLY_KEY {
        WEEKLY_WINDOW_SECONDS
    } else {
        FIVE_HOUR_WINDOW_SECONDS
    };
    let seconds = minutes.map_or(documented_seconds, |minutes| minutes * SECONDS_PER_MINUTE);
    Some(UsageWindow {
        key: format!("muse_{key}"),
        kind: window_kind_for_duration(seconds),
        label: humanize_window_seconds(seconds),
        utilization,
        resets_at,
    })
}
