//! The usage wire types: windows, credits, account readings and why a reading is missing.

use serde::Deserialize;
use serde::Serialize;

/// The two named top-level windows we give product names to; every other named
/// window the endpoint returns is surfaced generically as
/// [`UsageWindowKind::Opaque`].
const SESSION_WINDOW_KEY: &str = "five_hour";
const WEEKLY_WINDOW_KEY: &str = "seven_day";

/// The semantic kind of a rate-limit window. The endpoint returns a handful of
/// named windows (the 5-hour `five_hour` session, the 7-day `seven_day` weekly
/// all-models cap, the `extra_usage` monthly spend cap on usage credits) plus
/// per-model caps under `limits[]`; we classify each so the meter can label it and
/// the auto-resume scheduler can match the window a CLI named. A named window we
/// don't recognize passes through as `Opaque` - surfaced honestly, never dropped.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum UsageWindowKind {
    Session,
    Weekly,
    Model,
    Credits,
    Opaque,
}

impl UsageWindowKind {
    /// The one authoritative mapping from a top-level window key to its kind.
    pub(super) fn from_window_key(key: &str) -> Self {
        match key {
            SESSION_WINDOW_KEY => Self::Session,
            WEEKLY_WINDOW_KEY => Self::Weekly,
            CREDITS_KEY => Self::Credits,
            _ => Self::Opaque,
        }
    }

    /// Stable render + match order: session, then the weekly all-models cap, then
    /// per-model caps, then the credits spend cap, then anything unrecognized.
    pub(super) fn order(self) -> u8 {
        match self {
            Self::Session => 0,
            Self::Weekly => 1,
            Self::Model => 2,
            Self::Credits => 3,
            Self::Opaque => 4,
        }
    }
}

/// One rate-limit window surfaced from the account response: a stable `key`, its
/// semantic `kind`, a human `label`, the 0..100 `utilization` percent, and an
/// ISO-8601 reset time when known. The generic shape the meter renders and the
/// auto-resume scheduler matches against - one entry per window the endpoint
/// actually returns, nothing invented.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct UsageWindow {
    pub(crate) key: String,
    pub(crate) kind: UsageWindowKind,
    pub(crate) label: String,
    pub(crate) utilization: f64,
    pub(crate) resets_at: Option<String>,
}

/// The top-level key under which Claude's account response describes its
/// pay-as-you-go usage credits ("extra usage").
pub(super) const CREDITS_KEY: &str = "extra_usage";
/// A window or cap consumed this far has nothing left - also the scale every
/// `utilization` is on.
pub(super) const FULL_PERCENTAGE: f64 = 100.0;

/// The state of pay-as-you-go usage credits ("extra usage"). The auto-resume
/// scheduler reads it to continue a limit-stopped session the moment credits can
/// carry it - turned on server-side, which the CLI itself never notices.
#[derive(Serialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub(super) enum CreditsState {
    /// Turned off.
    Off,
    /// Turned on, with room left under the monthly cap (or no cap at all).
    Available,
    /// Turned on, but unable to carry work now: the monthly cap is spent, or the
    /// account names a reason they're held back.
    Unavailable,
}

/// The credits state an account response describes, or `None` when it carries no
/// credits object at all (every adapter but Claude's, or a Claude response that
/// omits it).
pub(super) fn credits_state(response: &serde_json::Value) -> Option<CreditsState> {
    let credits = response.get(CREDITS_KEY)?;
    let is_enabled = credits
        .get("is_enabled")
        .and_then(serde_json::Value::as_bool)?;
    if !is_enabled {
        return Some(CreditsState::Off);
    }

    let is_cap_spent = credits
        .get("utilization")
        .and_then(serde_json::Value::as_f64)
        .is_some_and(|utilization| utilization >= FULL_PERCENTAGE);
    let is_spend_limit_reached = credits
        .get("spend_limit_reached")
        .and_then(serde_json::Value::as_bool)
        .unwrap_or(false);
    let is_held_back = credits
        .get("disabled_reason")
        .is_some_and(serde_json::Value::is_string);
    if is_cap_spent || is_spend_limit_reached || is_held_back {
        return Some(CreditsState::Unavailable);
    }
    Some(CreditsState::Available)
}

/// An amount of usage credits - a prepaid balance, or a month's spend - ready
/// to show.
#[derive(Serialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub(super) struct CreditBalance {
    /// The amount in the currency's major units (dollars, not cents).
    pub(super) amount: f64,
    /// The ISO 4217 currency code (`USD`).
    pub(super) currency: String,
}

/// Stable identities for the underlying billing account behind each adapter, so
/// the frontend can dedupe agents that share one subscription (Codex + opencode
/// both bill the same `ChatGPT` account) - one identity per account, never per
/// agent.
#[derive(Serialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub(super) enum BillingAccount {
    Anthropic,
    Chatgpt,
    Github,
    Google,
    Cursor,
    Meta,
}

/// Live account usage mirrored from the OAuth endpoint: every rate-limit window it
/// returns (the session + weekly windows, any per-model caps, and any future
/// windows), plus the plan label. Its `Default` is the shape of an account with
/// no credits concept, which every adapter but Claude's builds on.
#[derive(Serialize, Clone, Debug, Default)]
#[serde(rename_all = "camelCase")]
pub struct AccountUsage {
    pub(crate) windows: Vec<UsageWindow>,
    pub(super) plan: String,
    pub(super) source: String,
    /// Stable identity of the underlying billing account, so two agents on one
    /// subscription are never counted twice. `None` when the adapter can't name
    /// the account.
    pub(super) account: Option<BillingAccount>,
    /// Whether usage credits are on and can carry work past an exhausted window.
    /// `None` for an account with no credits concept.
    pub(super) credits: Option<CreditsState>,
    /// The prepaid usage-credits balance. Read only while credits are on, so it is
    /// present exactly when extra usage is enabled (and the balance was reachable).
    pub(super) credit_balance: Option<CreditBalance>,
    /// What usage credits have cost this month when no monthly cap is set. With
    /// no cap there is no percentage to show, so the meter shows this instead.
    pub(super) uncapped_spend: Option<CreditBalance>,
}

/// Why an account has no reading - so the UI can say "sign-in expired" instead
/// of a bare dash, and tell the user what to do about it.
#[derive(Serialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub(crate) enum NoReadingReason {
    /// PADE has no usage source it can read for this agent.
    NoSource,
    /// The agent's sign-in isn't on disk.
    SignedOut,
    /// The vendor rejected the stored sign-in (a 401/403), or its recorded
    /// expiry has passed. The agent's CLI refreshes it on its next run.
    SignInExpired,
    /// The vendor didn't answer usably: no network, a refusal (429), a server
    /// error, or a body that isn't a usage response. Transient, so the last
    /// reading keeps standing in.
    Unreachable,
}

/// A failed usage request, as far as the reading cares: the vendor refused the
/// credentials, or it could not be reached / did not answer usably.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum RequestFailure {
    Rejected,
    Unreachable,
}

impl From<RequestFailure> for NoReadingReason {
    fn from(failure: RequestFailure) -> Self {
        match failure {
            RequestFailure::Rejected => Self::SignInExpired,
            RequestFailure::Unreachable => Self::Unreachable,
        }
    }
}

/// Why the latest ask brought no reading, and the CLI that fixes a sign-in
/// problem (`codex`), straight from the agent registry.
#[derive(Serialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub(crate) struct NoReading {
    pub(super) reason: NoReadingReason,
    pub(super) sign_in_command: Option<&'static str>,
}

/// What the account command answers: the reading to show - fresh, or the last
/// good one held over while the vendor fails (see [`LAST_READING_LIMIT`]) - and,
/// when the latest ask brought none, why. A fresh reading carries no
/// `no_reading`; a held-over one carries both.
#[derive(Serialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub(crate) struct AccountReading {
    pub(super) account: Option<AccountUsage>,
    pub(super) no_reading: Option<NoReading>,
}

impl AccountUsage {
    /// The billing account's identity as the wire names it (`anthropic`), or
    /// `None` when the adapter can't name the account - the key pacing shares an
    /// account's burn by.
    pub(crate) fn billing_identity(&self) -> Option<String> {
        let identity = serde_json::to_value(self.account?).ok()?;
        identity.as_str().map(ToString::to_string)
    }
}

impl AccountReading {
    /// The reading, when the latest ask brought one - for the backend's own
    /// consumers (the pacing engine) of what `usage_account_agent` answers.
    pub(crate) fn into_account(self) -> Option<AccountUsage> {
        self.account
    }

    /// The answer for an agent PADE has no usage adapter for.
    pub(super) const fn no_source() -> Self {
        Self {
            account: None,
            no_reading: Some(NoReading {
                reason: NoReadingReason::NoSource,
                sign_in_command: None,
            }),
        }
    }
}
