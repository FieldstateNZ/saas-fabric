//! What an identity provider currently holds, in the platform's own terms.

use std::collections::{BTreeMap, BTreeSet};

use fabric_client_model::{OidcClientId, RedirectUri, RoleName};

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
    /// The redirect URIs currently registered.
    pub redirect_uris: BTreeSet<RedirectUri>,

    /// Whether the provider holds it as a public client.
    ///
    /// Observed rather than assumed, because it is the one property that
    /// changes what a client *is*. A declared client that has been switched to
    /// confidential out of band has stopped matching its declaration in a way
    /// that breaks every browser flow using it, and the reconciler has to be
    /// able to see that.
    pub public: bool,
}
