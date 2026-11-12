//! Reading an adapter's stored sign-in: the credentials file and the fields a usable login must carry.

use super::readings::NoReadingReason;
use crate::util::read_json;
use std::path::PathBuf;

/// A credentials file an adapter reads, parsed - or [`NoReadingReason::SignedOut`]
/// when it is missing or unreadable, which is what a CLI that never signed in
/// (or signed out) leaves behind.
pub(super) fn read_sign_in(path: Option<PathBuf>) -> Result<serde_json::Value, NoReadingReason> {
    path.as_deref()
        .and_then(read_json)
        .ok_or(NoReadingReason::SignedOut)
}

/// A string the sign-in must carry (the token, the account id); its absence
/// means there is no usable sign-in.
pub(super) fn sign_in_field<'json>(
    json: &'json serde_json::Value,
    pointer: &str,
) -> Result<&'json str, NoReadingReason> {
    json.pointer(pointer)
        .and_then(serde_json::Value::as_str)
        .filter(|value| !value.is_empty())
        .ok_or(NoReadingReason::SignedOut)
}

/// A usage endpoint's body as JSON. One that doesn't parse is not a usage
/// response - the vendor didn't answer usably.
pub(super) fn parse_usage_body(body: &str) -> Result<serde_json::Value, NoReadingReason> {
    serde_json::from_str(body).map_err(|_| NoReadingReason::Unreachable)
}
