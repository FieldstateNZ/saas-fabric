//! An authenticated platform operator.

/// The human who made this request.
///
/// Holding one is a proof that the deployment's
/// [`OperatorAuthenticator`](super::OperatorAuthenticator) accepted the
/// request. There is no constructor outside this crate, and no way for a
/// handler to obtain one except by declaring it as a parameter — which is what
/// makes "did we check the operator?" a compile-time question rather than a
/// review question.
///
/// # Deliberately just a subject
///
/// No roles, no scopes, no per-client permissions. Every authenticated
/// operator may do everything this API offers, and pretending otherwise with
/// an authorisation model nothing enforces would be worse than the honest
/// limitation. What the subject *is* for is attribution: it names the person
/// in the audit record and in the Git commit that carries their change (§24).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Operator {
    /// How the identity provider named them, such as an email address.
    subject: String,
}

impl Operator {
    /// Builds an operator identity from an established subject.
    pub(crate) fn new(subject: impl Into<String>) -> Self {
        Self {
            subject: subject.into(),
        }
    }

    /// How the operator is named.
    #[must_use]
    pub fn subject(&self) -> &str {
        &self.subject
    }
}
