//! What an identity provider currently holds, in the platform's own terms.
//!
//! In the 121–150 line band: two small, closely related observed-state
//! structs — a realm and the one client shape it holds — each field
//! documented with the drift it represents. Splitting `ObservedOidcClient`'s
//! fields from each other, or from `ObservedRealm`, would separate one
//! observation from the reasons every part of it exists.

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
// A flag per attribute is the honest representation here: `public`, `enabled`,
// `standard_flow_enabled` and the post-logout term are four independent facts
// the provider reports, not four values of one state machine — a client can
// hold any combination of them, and `matches` needs each answered on its own.
// Grouping them into a sub-struct to satisfy the lint would add nesting at
// every call site and hide nothing (see `ConnectorCapabilities` for the same
// reasoning).
#[allow(clippy::struct_excessive_bools)]
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
    /// has exactly one.
    ///
    /// A client whose mapper has been removed, or never had one, reports
    /// `None` here — which is drift from the configured audience just as
    /// surely as a wrong string would be, because either way the edge's `aud`
    /// check refuses every token this client issues. `None` also covers a
    /// client carrying **more than one** audience mapper: this model never
    /// picks one arbitrarily among several, because which one the provider
    /// would treat as authoritative is exactly the ambiguity that should be
    /// visible as drift rather than resolved by a guess. The substantive
    /// proof that a provider actually behaves this way — mappers read back at
    /// all, and two of them collapsing to `None` rather than to "first
    /// found" — lives in `fabric-keycloak`'s
    /// `two_audience_mappers_are_observed_as_no_single_audience`; this crate
    /// only carries the resulting `Option`.
    pub audience_mapper: Option<String>,

    /// How many of the client's protocol mappers are not the one audience
    /// mapper this adapter writes.
    ///
    /// Client-level mappers only, never a client scope's: observed on
    /// Keycloak 26.0.8, a freshly written client carries exactly the one
    /// mapper. That boundary is also this field's limit: a mapper on a
    /// client scope assigned to the client — an audience mapper on a scope,
    /// say — is not observed here and is not drift this term can see. A mapper
    /// nobody declared — a hardcoded-claim mapper injecting a claim, say —
    /// added out of band is corrected the same way an absent or wrong
    /// audience mapper is: `declaration()` writes a client's entire
    /// mapper set and the provider's `PUT` replaces rather than merges it, so
    /// "a mapper nobody declared" is drift with the same fix as "the
    /// audience mapper is missing" — a full rewrite down to this adapter's
    /// own set. Without this field that extra mapper is invisible to every
    /// sweep, the same write/read asymmetry [`Self::audience_mapper`] closes
    /// for the mapper this platform does write.
    pub other_protocol_mappers: usize,

    /// How many of the provider's registered redirect URIs this model could
    /// not parse into a [`RedirectUri`].
    ///
    /// A count, not the values: an unparseable entry is attacker-influenced
    /// text with no reason to reach a plan, a log line, or an API response.
    /// Non-zero is drift regardless of what [`Self::redirect_uris`] holds — a
    /// client whose declared set is fully present *and* carries an extra,
    /// unmodellable entry has still drifted from its declaration.
    pub unmodellable_redirect_uris: usize,

    /// Whether the provider currently has the client enabled.
    ///
    /// A declared client is always enabled; one switched off by hand — through
    /// the provider's own console, never through this platform — answers
    /// nobody, silently, while every other field can still read as converged.
    pub enabled: bool,

    /// Whether the provider currently has the standard (authorization-code)
    /// flow enabled for this client.
    ///
    /// A declared client exists to run that flow (ADR 0019 §3); one with it
    /// switched off has stopped being able to authenticate anyone through it,
    /// from this platform's point of view without a word said about it.
    pub standard_flow_enabled: bool,

    /// Whether the provider's post-logout redirect setting still names
    /// "every registered redirect URI".
    ///
    /// Fabric always writes that setting as the literal value the provider
    /// uses as shorthand for the client's whole registered redirect set, so
    /// there is nothing here for this model to parse into URIs of its own —
    /// the adapter reports only whether the raw value is still that literal
    /// shorthand. An operator who narrows it by hand, to an explicit list or
    /// to nothing, is narrowing where a user can land after logging out, and
    /// that is drift the same way a redirect URI itself would be.
    pub post_logout_redirect_uris_is_every_registered_uri: bool,
}
