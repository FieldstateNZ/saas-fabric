//! Secret references, and the seam that resolves them.

use std::fmt;

use async_trait::async_trait;

use crate::ConnectorError;

/// A pointer to a credential, as it appears in a tenant's runtime binding.
///
/// Specification §21: secrets are never stored in Git tenant definitions, only
/// *referenced*. This is that reference — a logical path such as
/// `tenant/acme/data-primary`. What it resolves to (Azure Key Vault, AWS Secrets
/// Manager, HashiCorp Vault, a Kubernetes Secret) is a deployment concern that
/// no code above [`SecretResolver`] can observe.
///
/// The reference itself is not sensitive and may appear in logs. The *value* it
/// resolves to may never (§29).
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, serde::Serialize, serde::Deserialize)]
#[serde(transparent)]
pub struct SecretRef(String);

impl SecretRef {
    /// Wraps a secret reference path.
    #[must_use]
    pub fn new(path: impl Into<String>) -> Self {
        Self(path.into())
    }

    /// Borrows the reference path.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for SecretRef {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

/// A resolved credential value.
///
/// # Why this is not a `String`
///
/// It deliberately has a `Debug` implementation that prints nothing useful.
/// Secrets leak into telemetry by accident, not by design — someone adds
/// `?target` to a `tracing` field, or derives `Debug` on a struct three layers
/// up that happens to contain a credential, and a connection string ends up in
/// a log aggregator. §29 forbids that outright.
///
/// Making the type incapable of printing itself means the accident cannot
/// happen: reaching the secret requires calling [`ResolvedSecret::expose`],
/// which is greppable and obvious in review.
///
/// # No `Display`
///
/// `ResolvedSecret` also does not implement [`std::fmt::Display`], and never
/// should. `Debug` being redacted only closes one door — `{}` would still
/// print the raw value if a `Display` impl existed. Pinned below with a
/// doctest that must fail to compile, so an `impl Display for ResolvedSecret`
/// added later cannot slip past review unnoticed:
///
/// ```compile_fail
/// let secret = fabric_connector::ResolvedSecret::new("hunter2");
/// println!("{secret}"); // ResolvedSecret has no Display impl — must not compile.
/// ```
#[derive(Clone, PartialEq, Eq)]
pub struct ResolvedSecret(String);

impl ResolvedSecret {
    /// Wraps a resolved secret value.
    #[must_use]
    pub fn new(value: impl Into<String>) -> Self {
        Self(value.into())
    }

    /// Returns the secret value.
    ///
    /// Named to be conspicuous. Every call site should be somewhere a
    /// credential genuinely has to be used — building a connection string,
    /// setting an authorization header — and nowhere else.
    #[must_use]
    pub fn expose(&self) -> &str {
        &self.0
    }
}

impl fmt::Debug for ResolvedSecret {
    /// Prints a redaction marker, never the value.
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("ResolvedSecret(<redacted>)")
    }
}

/// Resolves a [`SecretRef`] to its value.
///
/// The implementation is a deployment concern. Applications never receive or
/// understand the physical secret location (§21), and neither does anything in
/// this crate above this trait.
///
/// Implementations are expected to cache. This sits on the request path when a
/// tenant uses [`ConnectionSelector::Secret`](crate::ConnectionSelector), and a
/// round trip to a secret store per data operation would be a poor trade.
#[async_trait]
pub trait SecretResolver: Send + Sync {
    /// Resolves a secret reference.
    ///
    /// # Errors
    ///
    /// Returns [`ConnectorError::SecretUnavailable`] when the reference cannot
    /// be resolved. This fails the request closed — there is no fallback
    /// credential (§28).
    async fn resolve(&self, reference: &SecretRef) -> Result<ResolvedSecret, ConnectorError>;
}
