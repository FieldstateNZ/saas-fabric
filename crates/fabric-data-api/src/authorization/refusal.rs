//! Why authorization did not permit an operation.

use crate::CheckFailure;

/// The outcome of a refused authorization.
///
/// Two shapes, because they answer the caller differently: a denial is a
/// statement about the caller's access, an undecided Check is a statement
/// about the platform. Both refuse.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Refusal {
    /// The caller is not permitted.
    Denied(DenialReason),

    /// Layer 2 was required and no decision was obtained (fail closed).
    Undecided(CheckFailure),
}

/// Which rule denied. Logged, never returned: every denial answers the caller
/// the same 403, so a status cannot say which layer refused.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum DenialReason {
    /// Layer 1: the token lacks the scope.
    MissingScope,

    /// Layer 2: the Check answered that the relation does not hold.
    CheckDenied,

    /// Layer 2 is required for a collection operation (`list`, `create`), and
    /// no model defines the collection object yet (ADR 0027 §2a). Nothing can
    /// allow it until that contract exists.
    CollectionContractUndefined,

    /// Layer 2 is required, and the key cannot be written as an object.
    UnnameableObject,
}

impl DenialReason {
    /// A stable name for the log line.
    pub(crate) const fn as_str(self) -> &'static str {
        match self {
            Self::MissingScope => "missing_scope",
            Self::CheckDenied => "check_denied",
            Self::CollectionContractUndefined => "collection_contract_undefined",
            Self::UnnameableObject => "unnameable_object",
        }
    }
}
