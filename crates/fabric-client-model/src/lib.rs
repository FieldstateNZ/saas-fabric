//! The control plane's desired-state model.
//!
//! A **client** is the commercial entity SaaS Fabric manages: Acme, who has a
//! name, some hostnames, and an identity configuration. This crate is the
//! model of that, plus the declarative document that carries it in Git.
//!
//! # Where this sits
//!
//! ```text
//! operator intent
//!       ↓
//! Control Plane API
//!       ↓
//! this crate            ← desired state, validated
//!       ↓
//! Git (saas-fabric-clients)
//!       ↓
//! reconciliation
//!       ↓
//! Keycloak / Envoy / OpenBao / …
//! ```
//!
//! It contains **no I/O**. It does not know what Git is, what Keycloak is, or
//! that an HTTP API exists. It knows what a client *should* look like and how
//! to read and write the document that says so.
//!
//! # Client and tenant are the same entity, deliberately not the same type
//!
//! [`ClientId`] and [`fabric_core::TenantId`] hold the same string for the
//! same organisation: client `acme` is tenant `acme` is realm `acme`. They are
//! separate types because they belong to different planes and are established
//! by different means — a `TenantId` comes from a request's bearer token
//! (§10), a `ClientId` comes from a path an operator addressed. Sharing one
//! type would make it possible to hand a runtime tenant identity to a control
//! plane operation, or the reverse, and neither should ever type-check. Both
//! validate with the same rule from [`fabric_core::naming`], so they cannot
//! disagree about which strings are legal.
//!
//! # Preserving what this model does not understand
//!
//! A client document carries more than identity: features, data placement,
//! configuration profile (see the platform specification §4). This increment
//! models identity only — so [`ClientDocument`] keeps the *entire* parsed
//! document and edits `spec.identity` within it, rather than round-tripping
//! through a struct that would silently drop every section it has no field
//! for. Editing a client's roles must not delete its feature flags.

mod authorization;
mod client;
mod document;
mod errors;
mod identity;
mod ids;
mod secrets;

pub use authorization::{AuthorizationConfiguration, Relation, ResourceAuthorization};
pub use client::Client;
pub use document::{ClientDocument, API_VERSION, API_VERSION_V2, KIND};
pub use errors::DesiredStateError;
pub use identity::{
    required_roles, AppScheme, ClientProtocol, IdentityConfiguration, OidcClient, PkceMethod,
    RedirectStrategy, RedirectStrategyKind, RedirectUri, RedirectUriKind, CUSTOM_SCHEME_PHASE,
};
pub use ids::{ClientId, ClientRevision, Host, OidcClientId, RealmName, RelationName, RoleName};
pub use secrets::{SecretNamespace, SecretsConfiguration};
