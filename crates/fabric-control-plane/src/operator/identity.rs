//! An authenticated platform operator, and the authority they carry.

/// The bearer an operator presented.
///
/// Held so that the platform can act on the identity provider **as them**
/// rather than as a service account of its own. That is the whole of ADR 0012:
/// permission to create a realm belongs to a human in the master realm, and
/// this is how their permission reaches the request that uses it.
///
/// No [`Display`](std::fmt::Display), and a [`Debug`] that prints a fixed
/// string. A bare `String` here is one `{:?}` on a request-scoped struct away
/// from putting an operator's access token into a log aggregator.
#[derive(Clone, PartialEq, Eq)]
pub struct OperatorToken(String);

impl OperatorToken {
    /// Wraps a presented bearer.
    pub(crate) fn new(value: impl Into<String>) -> Self {
        Self(value.into())
    }

    /// The token, for the one caller that has to present it upstream.
    #[must_use]
    pub fn expose(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Debug for OperatorToken {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("OperatorToken(redacted)")
    }
}

/// The human who made this request.
///
/// Holding one is a proof that the deployment's
/// [`OperatorAuthenticator`](super::OperatorAuthenticator) accepted the
/// request. There is no constructor outside this crate, and no way for a
/// handler to obtain one except by declaring it as a parameter — which is what
/// makes "did we check the operator?" a compile-time question rather than a
/// review question.
///
/// # A subject and an authority, both always present
///
/// The subject is for **attribution**: it names the person in the audit record
/// and in the Git commit that carries their change (§24). There are still no
/// roles or scopes here — every authenticated operator may do everything this
/// API offers, and pretending otherwise with an authorisation model nothing
/// enforces would be worse than the honest limitation.
///
/// The token is for **delegation**. The platform holds no authority over the
/// identity provider of its own, so when it creates a realm it does so as the
/// operator who asked (ADR 0012). That is why this is not optional: an
/// operator who could not lend an authority would be an operator half of this
/// API could not serve, and the posture that produced one was removed rather
/// than special-cased.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Operator {
    /// How the identity provider named them, such as an email address.
    subject: String,

    /// The bearer they presented, which the platform acts with.
    token: OperatorToken,
}

impl Operator {
    /// Builds an operator identity from a verified subject and their bearer.
    pub(crate) fn new(subject: impl Into<String>, token: OperatorToken) -> Self {
        Self {
            subject: subject.into(),
            token,
        }
    }

    /// How the operator is named.
    #[must_use]
    pub fn subject(&self) -> &str {
        &self.subject
    }

    /// The authority this operator lends the platform.
    #[must_use]
    pub fn token(&self) -> &OperatorToken {
        &self.token
    }
}
