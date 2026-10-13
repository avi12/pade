//! Assertions shared by the unit tests.

use std::fmt::Debug;

/// Fails showing what was there - a bare `assert!(items.is_empty())` only
/// says it wasn't empty.
#[track_caller]
pub fn assert_empty<T: Debug>(items: &[T]) {
    assert!(items.is_empty(), "expected nothing, got {items:?}");
}

/// The counterpart of [`assert_empty`], for symmetry at the call site.
#[track_caller]
pub fn assert_not_empty<T: Debug>(items: &[T]) {
    assert!(!items.is_empty(), "expected something, got nothing");
}
