//! A client's authorization configuration — who may do what to which resource.
//!
//! Read this as a SaaS Fabric concept, not an OpenFGA one. A resource, the
//! relations somebody can hold on it, and the operations each relation permits
//! are what an operator declares; that OpenFGA is the thing which ends up
//! holding them is the reconciler's business and nothing above it may assume
//! it (ADR 0008, ADR 0013).
//!
//! # What is declared here and what is not
//!
//! **The model, not the memberships.** This says that a `customers` resource
//! has `viewer`, `editor` and `owner` relations and what each permits. It does
//! not say that Alice is an editor of anything: that is a fact about tenant
//! data, it changes constantly, and routing it through a Git commit and a
//! reconciliation pass would be both slow and wrong. Desired state describes
//! the shape of authorization; the tuples that fill it in are runtime data.
//!
//! # Why it names the same resources the runtime plane serves
//!
//! [`LogicalResourceName`] is `fabric-core`'s, so the name an operator writes
//! here is the same name the Data API resolves against its catalogue — one
//! spelling, checked by one rule, on both sides of a boundary the two planes
//! are not allowed to cross directly.
//!
//! [`LogicalResourceName`]: fabric_core::LogicalResourceName

mod relation;
mod validation;
#[cfg(test)]
mod validation_tests;

pub use relation::{Relation, ResourceAuthorization};

/// What a client's authorization should look like.
///
/// # Why the list is ordered rather than a set
///
/// The same reason [`IdentityConfiguration`] gives: this is serialised back
/// into a Git document a human reads in a diff, and a set would reorder an
/// operator's list on every write. Uniqueness is enforced by
/// [`validate`](Self::validate) instead.
///
/// # Why an empty configuration is legitimate
///
/// A client that declares no resources here is a client whose authorization is
/// not managed by the platform yet — which is every client that existed before
/// this section did. It is absent from a document rather than empty in it, and
/// both read the same way, deliberately: adding a capability must not make
/// every document already in the repository unreadable.
///
/// [`IdentityConfiguration`]: crate::IdentityConfiguration
#[derive(Debug, Clone, PartialEq, Eq, Default, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AuthorizationConfiguration {
    /// The resources whose authorization this client declares.
    #[serde(default)]
    pub resources: Vec<ResourceAuthorization>,
}

impl AuthorizationConfiguration {
    /// Whether this client declares any authorization at all.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.resources.is_empty()
    }
}
