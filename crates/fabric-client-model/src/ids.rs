//! The validated names a client's desired state is made of.
//!
//! Every one of these is a newtype whose only constructor performs a full
//! character-set check, for the same reason the runtime plane's identifiers
//! are: these values are interpolated into URLs against the Keycloak admin
//! API, written into paths in a Git repository, and printed into commit
//! messages. Checking once, at the edge, is what lets the adapters treat them
//! as safe.

#[macro_use]
mod slug_newtype;

mod client_id;
mod client_revision;
mod host;
mod oidc_client_id;
mod realm_name;
mod relation_name;
mod role_name;
#[cfg(test)]
mod role_name_tests;

pub use client_id::ClientId;
pub use client_revision::ClientRevision;
pub use host::Host;
pub use oidc_client_id::OidcClientId;
pub use realm_name::RealmName;
pub use relation_name::RelationName;
pub use role_name::RoleName;
