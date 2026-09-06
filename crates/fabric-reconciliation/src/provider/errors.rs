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
/// # Why four variants and not one
///
/// Because each deserves a different response. `Unavailable` is worth
/// retrying and the next reconciliation pass will. `NotPermitted` means the
/// platform's own machine credential is wrong and retrying forever will not
/// fix it. `Rejected` means the desired state cannot be realised as written,
/// which is an operator's problem and not a transient one.
/// `NoAudienceConfigured` is different again: it is not a fault this
/// deployment can actually be in. A provider that failed to build reports
/// `Unavailable` from `observe_realm` and never reaches this check, and
/// `fabric-keycloak`'s own config requires the audience, so a provider that
/// built at all always has one to report. This variant exists for an
/// `IdentityProvider` implementation that answers `None` anyway — defence in
/// depth against a future or third-party adapter, not a scenario this
/// deployment produces today. Collapsing any of these would make one kind of
/// misconfiguration look like another, or like a restarting provider.
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

    /// The provider names no configured audience at all.
    ///
    /// Reported by [`IdentityProvider::configured_audience`](crate::IdentityProvider::configured_audience)
    /// returning `None`.
    /// [`IdentityReconciler::plan`](crate::IdentityReconciler::plan) checks
    /// this after a successful observation and refuses to build a plan at
    /// all, rather than comparing a declared client's audience mapper
    /// against nothing — see that method's rustdoc for why proceeding
    /// anyway would be worse than refusing.
    #[error("the provider names no audience")]
    NoAudienceConfigured,
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
