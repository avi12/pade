//! Cursor usage adapter (tier label only): confirms a local `cursor-agent` login.

use super::readings::{AccountUsage, BillingAccount, NoReadingReason};
use super::CURSOR_PLAN;
use crate::util::{home_directory, read_json};

// ---------------------------------------------------------------------------
// Cursor usage (tier-label only). The Cursor CLI (`cursor-agent`) stores a JWT
// under a version-dependent path (`~/.config/cursor/auth.json`,
// `~/.cursor/auth.json`, …). Cursor's usage does surface from `cursor.com/api/
// usage`, but only via a reverse-engineered `WorkosCursorSessionToken` *cookie*
// (userId::JWT) that a `CURSOR_API_KEY` can't even build, on an undocumented,
// version-fragile endpoint - too unreliable to poll. We confirm the local login
// exists and surface just the plan label.
// ---------------------------------------------------------------------------

/// Where the Cursor plan label comes from: a local login, wherever it was found.
const CURSOR_LOCAL_SOURCE: &str = "local:cursor";

/// The auth files `cursor-agent` has kept its login in, by version, under the
/// home directory. The layout drifts between versions, so every one is probed.
const CURSOR_AUTH_RELATIVE_PATHS: [&str; 5] = [
    ".config/cursor/auth.json",
    ".cursor/auth.json",
    ".config/cursor-agent/auth.json",
    "AppData/Roaming/cursor-agent/auth.json",
    ".cursor/cli-config.json",
];

/// Confirm a Cursor login exists on disk (or a `CURSOR_API_KEY` override) and
/// surface a bare plan label with no numbers. Signed out when no login is
/// present.
pub(super) fn fetch_cursor_account_usage() -> Result<AccountUsage, NoReadingReason> {
    if !is_cursor_login_present() {
        return Err(NoReadingReason::SignedOut);
    }
    Ok(AccountUsage {
        plan: CURSOR_PLAN.to_string(),
        source: CURSOR_LOCAL_SOURCE.to_string(),
        account: Some(BillingAccount::Cursor),
        ..AccountUsage::default()
    })
}

/// Whether a Cursor login is present: the documented `CURSOR_API_KEY` override,
/// or a stored session in one of the [`CURSOR_AUTH_RELATIVE_PATHS`] (a JWT under
/// `accessToken`/`access_token`, or the `authInfo` marker the CLI writes to
/// `cli-config.json`). A wrong guess simply yields "not present".
fn is_cursor_login_present() -> bool {
    if std::env::var("CURSOR_API_KEY").is_ok_and(|key| !key.is_empty()) {
        return true;
    }
    let Some(home) = home_directory() else {
        return false;
    };
    CURSOR_AUTH_RELATIVE_PATHS
        .iter()
        .any(|relative_path| auth_file_has_login(&home.join(relative_path)))
}

/// Whether an auth file names a stored login - a non-empty `accessToken`/
/// `access_token` JWT, or the `authInfo` block the CLI writes once signed in.
fn auth_file_has_login(path: &std::path::Path) -> bool {
    let Some(json) = read_json(path) else {
        return false;
    };
    let has_named_token = ["accessToken", "access_token"].iter().any(|key| {
        json.get(key)
            .and_then(serde_json::Value::as_str)
            .is_some_and(|token| !token.is_empty())
    });
    has_named_token || json.get("authInfo").is_some()
}
