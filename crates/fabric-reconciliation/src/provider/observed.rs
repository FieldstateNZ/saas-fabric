//! What an identity provider currently holds, in the platform's own terms.

use std::collections::{BTreeMap, BTreeSet};

use fabric_client_model::{OidcClientId, PkceMethod, RedirectUri, RoleName};

/// A realm as it currently exists.
///
/// # Only what the platform declares
///
/// A real realm has dozens of settings SaaS Fabric says nothing about — token
/// lifespans, brute-force policy, themes, every default role the provider
/// created for itself. None of them appear here, and that is what makes
/// "reconciliation only adds" honest: the reconciler cannot notice a
/// difference in a field it cannot see, so it cannot decide to overwrite an
/// operator's deliberate change to one.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ObservedRealm {
    /// The realm's display name.
    pub display_name: String,

    /// Every realm role the provider reports, including any it created
    /// itself.
    ///
    /// A set rather than a list, because the only question asked of it is
    /// whether a desired role is present.
    pub roles: BTreeSet<RoleName>,

    /// The application clients the provider reports, keyed by the id an
    /// application presents.
    pub clients: BTreeMap<OidcClientId, ObservedOidcClient>,
}

/// An application client as it currently exists.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ObservedOidcClient {
    /// The redirect URIs currently registered, limited to the ones this model
    /// can parse. See [`Self::unmodellable_redirect_uris`] for the rest.
    pub redirect_uris: BTreeSet<RedirectUri>,

    /// Whether the provider holds it as a public client.
    ///
    /// Observed rather than assumed, because it is the one property that
    /// changes what a client *is*. A declared client that has been switched to
    /// confidential out of band has stopped matching its declaration in a way
    /// that breaks every browser flow using it, and the reconciler has to be
    /// able to see that.
    pub public: bool,

    /// The PKCE challenge method the provider currently enforces, as this
    /// model understands it.
    ///
    /// `None` covers both "the provider holds no such setting" and "the
    /// provider holds a value this model does not recognise" — `plain`, an
    /// empty string, a typo. Either way it is not `Some(S256)`, which is all
    /// the comparison needs: no `Plain` variant has to exist anywhere in this
    /// model for a downgrade to be seen as drift.
    pub challenge_method: Option<PkceMethod>,

    /// The audience the provider's audience mapper currently asserts, if it
    /// has one.
    ///
    /// A client whose mapper has been removed, or never had one, reports
    /// `None` here — which is drift from the configured audience just as
    /// surely as a wrong string would be, because either way the edge's `aud`
    /// check refuses every token this client issues.
    pub audience_mapper: Option<String>,

    /// How many of the provider's registered redirect URIs this model could
    /// not parse into a [`RedirectUri`].
    ///
    /// A count, not the values: an unparseable entry is attacker-influenced
    /// text with no reason to reach a plan, a log line, or an API response.
    /// Non-zero is drift regardless of what [`Self::redirect_uris`] holds — a
    /// client whose declared set is fully present *and* carries an extra,
    /// unmodellable entry has still drifted from its declaration.
    pub unmodellable_redirect_uris: usize,
}
