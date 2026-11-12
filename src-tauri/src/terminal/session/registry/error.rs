//! The typed failure of a session operation.

use std::sync::PoisonError;

use serde::{Deserialize, Serialize};

/// Why a session operation failed. Typed, because callers branch on it (a
/// duplicate spawn that lost the race reports the owner check of the session
/// that won), and serializable, because it crosses the PTY host's protocol.
/// The messages are what the frontend has always been shown.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(
    tag = "kind",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum SessionError {
    /// The session belongs to another window.
    Foreign,
    /// The calling window already holds its maximum number of sessions.
    WindowFull,
    /// A move would take the receiving window past its maximum.
    ReceivingWindowFull,
    /// A move named a session that is not live.
    Missing { id: String },
    /// The operating system, the PTY or a poisoned lock refused.
    Failed { message: String },
}

impl SessionError {
    pub(crate) fn failed(error: impl std::fmt::Display) -> Self {
        Self::Failed {
            message: error.to_string(),
        }
    }
}

impl std::fmt::Display for SessionError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Foreign => formatter.write_str("PTY belongs to another window"),
            Self::WindowFull => {
                formatter.write_str("this window has reached its PTY session limit")
            }
            Self::ReceivingWindowFull => {
                formatter.write_str("the receiving window has reached its PTY session limit")
            }
            Self::Missing { id } => write!(formatter, "no live session {id} to move"),
            Self::Failed { message } => formatter.write_str(message),
        }
    }
}

impl std::error::Error for SessionError {}

impl<T> From<PoisonError<T>> for SessionError {
    fn from(error: PoisonError<T>) -> Self {
        Self::failed(error)
    }
}
