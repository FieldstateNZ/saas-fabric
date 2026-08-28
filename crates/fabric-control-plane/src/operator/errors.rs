//! Why a request could not be attributed to an operator.

/// Why an operator identity could not be established.
///
/// Both variants fail the request closed. There is no anonymous mode, no
/// default operator, and no configuration that turns this off — a control
/// plane that can be reached without an identity has no audit trail worth the
/// name (§24).
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum OperatorAuthError {
    /// The request carried no operator identity at all.
    ///
    /// In the trusted-header posture this almost always means the request did
    /// not come through the operator-plane proxy — which is the case worth
    /// noticing, because it is either a misconfiguration or someone reaching
    /// the service directly.
    #[error("request carries no operator identity")]
    Missing,

    /// An identity was presented, but it is not one of this platform's
    /// operators.
    ///
    /// The subject is deliberately not echoed back. It is attacker-controlled
    /// in the case that matters, and reflecting it would turn the error body
    /// into a mirror. It is logged instead.
    #[error("not a platform operator")]
    NotAnOperator,
}
