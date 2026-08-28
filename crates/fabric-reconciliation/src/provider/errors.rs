//! Why an identity provider could not do what was asked.

/// A failure reported by an identity provider adapter.
///
/// # What an adapter may put in `detail`
///
/// A short, adapter-authored description: which operation, and the HTTP status
/// class if that is the useful fact. **Never an upstream response body**, and
/// never a credential, token, or `Authorization` header. This text reaches a
/// log line and, in sanitised form, an operator's screen — an adapter that
/// forwarded Keycloak's own error body would put realm internals and
/// occasionally a token fragment in both.
///
/// # Why three variants and not one
///
/// Because the three deserve different responses. `Unavailable` is worth
/// retrying and the next reconciliation pass will. `NotPermitted` means the
/// platform's own machine credential is wrong and retrying forever will not
/// fix it. `Rejected` means the desired state cannot be realised as written,
/// which is an operator's problem and not a transient one. Collapsing them
/// would make a misconfigured credential look exactly like a restarting
/// provider.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ProviderError {
    /// The provider could not be reached, or failed internally.
    #[error("the identity provider is unavailable: {detail}")]
    Unavailable {
        /// What the adapter observed, with no upstream body in it.
        detail: String,
    },

    /// The platform's own administrative credential was refused.
    #[error("the identity provider refused the platform's administrative credential")]
    NotPermitted,

    /// The provider refused the request as invalid.
    #[error("the identity provider rejected the request: {detail}")]
    Rejected {
        /// What the adapter observed, with no upstream body in it.
        detail: String,
    },
}

impl ProviderError {
    /// Whether another attempt could plausibly succeed without anyone
    /// intervening.
    ///
    /// Used to decide what a failed reconciliation is *called*, not whether to
    /// retry — the reconciliation loop runs on a schedule and retries
    /// everything either way.
    #[must_use]
    pub const fn is_transient(&self) -> bool {
        matches!(self, Self::Unavailable { .. })
    }
}
