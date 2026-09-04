//! Wiring the transport decision into `reqwest`'s own redirect policy.

use super::{decide, Transport};

/// Carries a refusal reason through `reqwest`'s own redirect-error channel,
/// so a caller can recognise a policy refusal via `reqwest::Error::is_redirect`
/// without this reader inventing a second channel for the same information.
#[derive(Debug)]
struct RedirectRefused(String);

impl std::fmt::Display for RedirectRefused {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(&self.0)
    }
}

impl std::error::Error for RedirectRefused {}

/// Builds the redirect policy for `transport`.
///
/// `transport` is `Copy`, so the closure captures it by value rather than
/// borrowing — satisfying the `Fn + Send + Sync + 'static` bound
/// `reqwest::redirect::Policy::custom` requires without a lifetime to carry.
pub(in crate::charts) fn policy(transport: Transport) -> reqwest::redirect::Policy {
    reqwest::redirect::Policy::custom(move |attempt| {
        match decide(transport, attempt.previous(), attempt.url()) {
            Ok(()) => attempt.follow(),
            Err(reason) => attempt.error(RedirectRefused(reason)),
        }
    })
}
