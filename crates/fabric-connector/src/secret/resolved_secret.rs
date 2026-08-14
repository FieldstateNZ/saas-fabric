//! A resolved credential value, and the reasons it cannot print itself.

use std::fmt;

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
