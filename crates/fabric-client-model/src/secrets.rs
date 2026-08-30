//! Where a client's secrets live.
//!
//! Read this as a SaaS Fabric concept. A client has a secret boundary; that
//! OpenBao implements it as a namespace is the adapter's business, and the
//! word "namespace" appears here only because it is what the platform has
//! already chosen as the boundary — not because anything above the adapter
//! may assume the store.
//!
//! # Why this is declared rather than derived
//!
//! The obvious shortcut is `namespace = client id`. It is almost always true
//! and it is the wrong contract: a boundary that is only ever implied cannot
//! be read out of the document, cannot differ for one client that needs it to,
//! and cannot be changed without a migration nobody can see. The realm is
//! stated for the same reason (ADR 0013).
//!
//! # Why a caller can never supply it
//!
//! This value is trusted infrastructure state. An operator says *which client*
//! and *which path within it*; the boundary is resolved from here. A request
//! that could name its own boundary is a request that can read another
//! client's secrets, which is the whole thing this type exists to prevent.

mod namespace;
#[cfg(test)]
mod secrets_tests;

pub use namespace::SecretNamespace;

/// Where a client's secrets are kept.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SecretsConfiguration {
    /// The boundary every one of this client's secrets lives inside.
    pub namespace: SecretNamespace,
}
