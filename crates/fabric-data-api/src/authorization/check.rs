//! Layer 2's port: the relation Check, as the Data API asks it.

use async_trait::async_trait;

use crate::{CallerCredential, CheckTarget, OperationKind};

/// What the Data API asks: may the caller perform `operation` on `target`?
///
/// The operation, not a relation name. ADR 0013 puts the translation from
/// operations to the authorization model's computed relations in the adapter,
/// so this port speaks the platform's vocabulary and only the adapter speaks
/// the model's.
///
/// There is no subject here. The subject is whoever the credential belongs
/// to, and the authorization front binds it from the verified token
/// (ADR 0016); a field naming one would be a field somebody could fill in.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CheckQuestion {
    /// The operation attempted.
    pub operation: OperationKind,

    /// The record or collection it is attempted on.
    pub target: CheckTarget,
}

/// A decision the authorization service actually made.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CheckDecision {
    /// The relation holds.
    Allowed,
    /// The relation does not hold.
    Denied,
}

/// Why no decision was obtained.
///
/// None of these is a denial, and none is a permission: every one refuses the
/// request (ADR 0027, fail closed). They are kept apart only so the caller is
/// told the truth about whether retrying could help.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum CheckFailure {
    /// The service did not answer: refused, reset, timed out, or `5xx`.
    #[error("the authorization service is unavailable")]
    Unavailable,

    /// The service answered something that is not a decision — a status or
    /// body the platform did not expect. The platform is wrong, not the caller.
    #[error("the authorization service returned something other than a decision")]
    Internal,

    /// No authorization service is configured in this deployment, yet a tenant
    /// policy requires a Check.
    #[error("no authorization service is configured")]
    NotConfigured,
}

/// Asks the authorization service a relation question.
///
/// A port so layer 2's selection and fail-closed rules can be tested without a
/// service, and so the only code that speaks the front's protocol is the
/// adapter behind it.
#[async_trait]
pub trait AuthorizationCheck: Send + Sync {
    /// Asks the question about the holder of `credential`.
    ///
    /// # Errors
    ///
    /// A [`CheckFailure`] whenever no decision was obtained. An implementation
    /// must never turn a failure into [`CheckDecision::Allowed`], and should
    /// not turn one into [`CheckDecision::Denied`] either: a denial tells the
    /// caller their access is wrong when it is the platform that is.
    async fn check(
        &self,
        credential: &CallerCredential,
        question: &CheckQuestion,
    ) -> Result<CheckDecision, CheckFailure>;
}
