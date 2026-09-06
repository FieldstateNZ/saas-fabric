//! A client's identity configuration — the first capability SaaS Fabric
//! manages end to end.
//!
//! Read this as a SaaS Fabric concept, not a Keycloak one. A realm, a set of
//! realm roles and a set of application clients are what an operator declares;
//! that Keycloak is the thing which ends up holding them is the reconciler's
//! business and nothing above it may assume it (ADR 0008).

mod client_rules;
#[cfg(test)]
mod client_rules_tests;
mod oidc_client;
mod pkce_method;
mod redirect_strategy;
mod redirect_uri;
pub mod required_roles;
mod validation;
#[cfg(test)]
mod validation_tests;

use crate::{RealmName, RoleName};

pub use client_rules::CUSTOM_SCHEME_PHASE;
pub use oidc_client::{ClientProtocol, OidcClient};
pub use pkce_method::PkceMethod;
pub use redirect_strategy::{RedirectStrategy, RedirectStrategyKind};
pub use redirect_uri::{AppScheme, RedirectUri, RedirectUriKind};

/// What a client's identity should look like.
///
/// # Why the lists are ordered rather than sets
///
/// `roles` and `clients` are `Vec`, not `BTreeSet`, because this type is
/// serialised straight back into a Git document a human reads and reviews in a
/// diff. A set would silently reorder an operator's list on every write,
/// turning a one-role change into a whole-file diff. Uniqueness is enforced by
/// [`validate`](Self::validate) instead, which is the same guarantee arriving
/// by a route that leaves the document alone.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct IdentityConfiguration {
    /// The realm this client's users and applications belong to.
    ///
    /// Conventionally the client id, but stated explicitly rather than
    /// derived: a realm that is only ever implied cannot be read out of the
    /// document, and reconciliation has to name it.
    pub realm: RealmName,

    /// The realm roles that must exist.
    ///
    /// Must include every entry in
    /// [`required_roles::REQUIRED_ROLES`] — the platform's contract with a
    /// client depends on those two existing, so removing one is refused
    /// rather than reconciled.
    pub roles: Vec<RoleName>,

    /// The application clients that must exist in the realm.
    ///
    /// May be empty: a client whose applications are not yet declared is a
    /// legitimate intermediate state, not an error.
    #[serde(default)]
    pub clients: Vec<OidcClient>,
}
