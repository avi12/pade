//! Antigravity usage adapter (tier label only): confirms the local Google login.

use super::readings::{AccountUsage, BillingAccount, NoReadingReason};
use super::sign_in::{read_sign_in, sign_in_field};
use super::{local_source, ANTIGRAVITY_PLAN};
use crate::util::home_directory;

// ---------------------------------------------------------------------------
// Antigravity usage (tier-label only). Google's Antigravity CLI stores a Google
// OAuth token under `~/.gemini/antigravity-cli/antigravity-oauth-token`. Its
// remaining-quota does surface from a private `cloudcode-pa.googleapis.com`
// `v1internal:fetchAvailableModels` call, but that endpoint is undocumented,
// reverse-engineered, POST-only, and known to report headroom while generation
// is already rate-limited (a number that would mislead the meter) - so we do NOT
// poll it. We confirm the local login exists and surface just the plan label.
// ---------------------------------------------------------------------------

/// The Antigravity CLI's stored Google login, under the home directory.
const ANTIGRAVITY_TOKEN_RELATIVE_PATH: &str = ".gemini/antigravity-cli/antigravity-oauth-token";

/// Confirm the Antigravity OAuth token exists on disk and surface a bare plan
/// label with no numbers (no quota-free usage endpoint we trust). Signed out
/// when no login is present.
pub(super) fn fetch_antigravity_account_usage() -> Result<AccountUsage, NoReadingReason> {
    let json =
        read_sign_in(home_directory().map(|home| home.join(ANTIGRAVITY_TOKEN_RELATIVE_PATH)))?;
    sign_in_field(&json, "/token/access_token")?;

    Ok(AccountUsage {
        plan: ANTIGRAVITY_PLAN.to_string(),
        source: local_source(ANTIGRAVITY_TOKEN_RELATIVE_PATH),
        account: Some(BillingAccount::Google),
        ..AccountUsage::default()
    })
}
