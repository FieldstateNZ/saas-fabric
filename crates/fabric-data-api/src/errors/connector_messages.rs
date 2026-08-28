//! What a caller is told about a connector failure.
//!
//! Every sentence here is a constant compiled into this crate. Connector text
//! names physical tables, schemas, and servers (§2, §29), so none of it is
//! relayed — which leaves the wording one job, and it is not politeness. It has
//! to be right about the *effect*: a caller told "could not be executed" about
//! a write that landed will duplicate it, and one told "it was applied" about a
//! write that never left will lose it. Both are worse than the status code.

use fabric_connector::{ConnectorError, OperationEffect};

/// A failure whose detail stays internal and whose effect gives the caller
/// nothing to act on.
pub(super) const MASKED: &str = "internal error";

/// Provably not delivered, so a retry repeats an attempt rather than an effect.
pub(super) const NOT_DELIVERED: &str =
    "the backend was not reached, so this request was not carried out; it is safe to retry";

/// A read that broke after the request went out. Nothing was mutated, so where
/// on the wire it broke does not change what the caller should do.
pub(super) const READ_INCOMPLETE: &str =
    "the request could not be completed and nothing was changed; it is safe to retry";

/// A write that went out and was never answered. The one message that tells a
/// caller to go and look, because the platform genuinely does not know.
pub(super) const WRITE_UNKNOWN: &str =
    "the backend was reached but did not answer, so this write may or may not have been applied; read \
     the current state before retrying, because this request is not idempotent";

/// A write the backend confirmed with a success status, whose result was then
/// lost. The rows are in; only the count is gone.
pub(super) const WRITE_APPLIED: &str =
    "the write was applied, but its result could not be read, so the number of affected records is \
     unavailable; do not retry";

/// A refusal the caller could act on, whose specifics are still not theirs.
pub(super) const NOT_EXECUTED: &str = "the request could not be executed";

/// A write the platform can prove never ran.
///
/// Deliberately promises nothing about a retry *succeeding* — a request the
/// backend would not accept will be refused again — only that repeating it
/// cannot double-apply anything, which is the question a caller holding a
/// non-idempotent write is actually asking.
pub(super) const WRITE_NOT_APPLIED: &str =
    "this write was not carried out, so no records were changed; retrying cannot duplicate it";

/// What a masked internal failure may still tell a caller about their write.
///
/// A read gets the bare mask: it changed nothing, so there is nothing to be
/// honest about beyond refusing to relay the detail. A write gets a sentence
/// chosen by [`ConnectorError::effect`], because the one thing a caller must
/// not be left to guess is whether their mutation happened — and two of the
/// failures that land here did happen. Asking `effect()` rather than listing
/// variants means one added to `ConnectorError` later is described honestly
/// without anyone remembering to revisit this file.
pub(super) const fn internal(error: &ConnectorError, writing: bool) -> &'static str {
    if !writing {
        return MASKED;
    }

    match error.effect() {
        // Only ever built after a success status, so the write landed.
        OperationEffect::Applied => {
            "the write was carried out, but the backend's answer could not be understood, so the result \
             is unavailable; do not retry"
        }
        // A refusal the platform cannot date: the backend either never answered
        // or answered with a status that does not say when it stopped.
        OperationEffect::Unknown => {
            "the backend refused this write and the platform cannot tell whether it was applied first; \
             read the current state before retrying"
        }
        // Provably never ran: refused before the request was built, or refused
        // by a backend that declined the request rather than failing part-way
        // through it. `MASKED` would be a downgrade here — this is the one
        // masked outcome where the caller learns something worth having, and it
        // is true of every variant that reaches this arm.
        OperationEffect::NotApplied => WRITE_NOT_APPLIED,
    }
}
