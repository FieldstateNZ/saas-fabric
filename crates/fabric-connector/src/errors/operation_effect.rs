//! What a failure says about whether the operation took effect.

use crate::errors::rejection_effect;
use crate::ConnectorError;

/// What is known about whether a failed operation took effect on the backend.
///
/// # Why this exists
///
/// Every failure here is a failure to *learn the result*. That is not the same
/// as a failure to *have an effect*, and for a non-idempotent write the
/// difference is the whole question. `POST /v1/data/{resource}` carries up to
/// 500 rows and no idempotency key, so a retry is a second write, not a second
/// attempt at the first one.
///
/// Answering it needs a fact from the transport that gets thrown away if every
/// HTTP failure is collapsed into one variant: **where in the exchange it
/// broke**. A refused connect happens before any request byte is written. A
/// timeout can fire after the whole body is on the wire. A body that stops
/// arriving after `200 OK` means the backend already committed and said so.
/// Those are three different answers, and a caller that cannot tell them apart
/// has to assume the worst about all three — or, far worse, assume the best,
/// which is what mapping all of them to a retryable status does.
///
/// # This is not a retry policy
///
/// It states what is known, not what to do. Whether [`Unknown`](Self::Unknown)
/// is safe to retry depends on the operation: for a read it always is, for an
/// insert it never is without an idempotency key the platform does not yet
/// have. The layer that knows which operation was being run is the layer that
/// decides — this type only makes sure it has the fact it needs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OperationEffect {
    /// The operation certainly did not take effect.
    ///
    /// It was refused before anything was sent, or the request never reached
    /// the backend. Retrying it repeats an attempt, not an effect.
    NotApplied,

    /// The operation may or may not have taken effect.
    ///
    /// The honest answer whenever the request went out and no conclusive answer
    /// came back. A caller holding this cannot retry a non-idempotent write
    /// without risking a duplicate; it has to read the current state first.
    Unknown,

    /// The operation certainly took effect, and only its result was lost.
    ///
    /// The backend answered with a success status. Retrying applies it twice.
    Applied,
}

impl ConnectorError {
    /// What this failure says about whether the operation took effect.
    ///
    /// See [`OperationEffect`] for why the distinction is load-bearing.
    ///
    /// Only `Rejected` needs to look at anything beyond its own variant: it is
    /// the one failure the backend *answered*, so what it answered decides.
    /// [`rejection_effect`](crate::rejection_effect) owns that reading, and it
    /// is narrower than "a 4xx did not apply" — read it before assuming which
    /// rejections are conclusive.
    #[must_use]
    pub const fn effect(&self) -> OperationEffect {
        match self {
            // All refused locally, before any request was made.
            Self::UnknownConnector(_)
            | Self::Unsupported { .. }
            | Self::UnknownCollection(_)
            | Self::SecretUnavailable { .. }
            | Self::InvalidOperation(_)
            | Self::Unreachable { .. } => OperationEffect::NotApplied,

            Self::OutcomeUnknown { .. } => OperationEffect::Unknown,

            // A success status was read off the wire before either of these was
            // built. Two places build them, and both sit after that point:
            // `client::response_decoding`, which returns `Rejected` for any
            // non-success status and so can only reach these arms on a 2xx; and
            // `fabric-data-api`'s write-integrity check, which raises
            // `MalformedResponse` when a completed mutation reports more rows
            // affected than were sent. So the backend ran the operation and
            // reported that it worked; what failed was reading, or believing,
            // what it returned.
            //
            // The count matters here: an earlier version of this comment named
            // one place, and the second appeared without it being revisited. If
            // a third arrives, this arm must be re-argued rather than assumed —
            // the classification is only sound while every producer is
            // downstream of a success status.
            Self::ResultLost { .. } | Self::MalformedResponse { .. } => OperationEffect::Applied,

            // The backend answered, so its answer is evidence — but only for
            // the two statuses that mean it declined the request rather than
            // failed part-way through executing it.
            Self::Rejected { status, .. } => rejection_effect(*status),
        }
    }
}
