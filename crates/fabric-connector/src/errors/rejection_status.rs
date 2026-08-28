//! Reading a rejection's status code as a statement about the write.
//!
//! # Why a raw HTTP status is in a protocol-neutral crate
//!
//! `scripts/check_architecture.py` lists `fabric-connector` among the domain
//! crates that may not know what HTTP is, and ADR 0001 keeps the NDC protocol
//! inside `fabric-connector-ndc`. A bare `u16` does not trip that check — it
//! looks for HTTP *crates* — so what governs here is the spirit of the rule,
//! and it was weighed rather than sidestepped.
//!
//! The alternative was to carry the verdict instead of the number: let
//! `fabric-connector-ndc` map status to [`OperationEffect`] at the protocol
//! boundary, where protocol knowledge belongs, leaving this crate holding a
//! value it cannot misread. Two things decided it the other way.
//!
//! First, the status has worth this crate cannot reconstruct. An operator
//! reading a log needs to know a write was refused `409` rather than `422`; a
//! pre-classified `Unknown` has discarded that before the log line is written,
//! and no amount of message text puts it back.
//!
//! Second, and the stronger reason: the interpretation is a *platform*
//! judgement, not a connector one. It decides what a caller is told about their
//! own data, and the argument for it — set out on [`rejection_effect`] — is
//! long, narrower than it first appears, and easy to get wrong in the direction
//! that loses data. Devolving it to each connector crate means two connectors
//! can answer differently for the same status, and whoever writes the second
//! one will not have read this. One rule in one place outweighs a tidier
//! dependency list.
//!
//! # What a second, non-HTTP protocol would require
//!
//! A connector protocol that is not HTTP — gRPC, a message bus, an in-process
//! adapter — has no `u16` to supply, and minting a plausible-looking one would
//! be a guess wearing a status code's clothes. Such a protocol must not widen
//! [`rejection_effect`]. It should turn the field into a small neutral enum
//! that HTTP and the newcomer both map *into*, with this rule demoted to the
//! HTTP arm of that mapping. That is a larger change than one protocol repays,
//! and it is the right change for two.

use crate::OperationEffect;

/// What a backend's rejection status says about whether the operation ran.
///
/// Only two codes are conclusive. Read the reasoning before widening it:
/// answering [`NotApplied`](OperationEffect::NotApplied) for a write that did
/// land tells a caller their records are absent when they are present, which is
/// worse than the [`Unknown`](OperationEffect::Unknown) it replaces.
///
/// # What the NDC specification says
///
/// NDC v0.2.13 `specification/error-handling.md` defines eight codes. Four are
/// 4xx, and they disagree about *when* the failure happens:
///
/// - **400 Bad Request** — the request did not match the connector's
///   expectation based on the specification. A protocol-shape failure, settled
///   by reading the request.
/// - **422 Unprocessable Content** — the request was well-formed but not
///   semantically correct; the example given is a custom scalar supplied with
///   the wrong type. Settled by validating the request's values.
/// - **403 Forbidden** — a permission check failed; the example given is a
///   mutation failing because a check constraint was not met.
/// - **409 Conflict** — the request would create a conflicting state; the
///   example given is a mutation failing a foreign key constraint.
///
/// The first two describe a connector declining to act on the request at all.
/// The last two describe a mutation that reached the data source and was
/// stopped by it — a failure *during* execution.
///
/// # Why rollback does not rescue 403 and 409
///
/// The specification's only statement about partial application sits in
/// `mutations/README.md` under `### Multiple Operations`: if any operation
/// fails, none of them should effect any changes to the data source. Three
/// things disqualify it as a guarantee here. It is gated on the
/// `mutation.transactional` capability and scoped to requests carrying more
/// than one operation, and this platform always sends exactly one, so the
/// clause never applies to it. It is phrased as intent — "ought to", "should" —
/// rather than as a requirement. And the word "atomic" appears nowhere in the
/// specification source.
///
/// For a single operation the specification says nothing at all. A procedure is
/// opaque: `MutationOperationResults` carries one `type` and one free-form
/// `result`, with no per-row status, no error variant and no affected count, so
/// a connector could not report a partial application even if it wanted to. A
/// procedure that writes some rows and then trips a constraint may answer 409
/// with those rows committed and still conform.
///
/// # Everything else
///
/// Undefined statuses stay [`Unknown`](OperationEffect::Unknown) for that
/// reason and one more: they may come from an intermediary the connector never
/// saw. A proxy's `408` is 4xx and says nothing about the backend, and a `429`
/// may be raised by a sidecar either before or after the request was forwarded.
#[must_use]
pub const fn rejection_effect(status: u16) -> OperationEffect {
    match status {
        // The two the specification defines as refusals of the request itself.
        400 | 422 => OperationEffect::NotApplied,
        _ => OperationEffect::Unknown,
    }
}
