//! The Keycloak adapter: where SaaS Fabric's identity concepts become
//! Keycloak's.
//!
//! # The boundary this crate is
//!
//! ```text
//! IdentityReconciler          realm, role, application client
//!         ↓
//! IdentityProvider            the port — still SaaS Fabric's words
//!         ↓
//! KeycloakIdentityProvider    ← the translation happens here, and only here
//!         ↓
//! Keycloak Admin REST API     RealmRepresentation, ClientRepresentation, …
//! ```
//!
//! **Keycloak types stop here.** `RealmRepresentation`, `RoleRepresentation`,
//! `ClientRepresentation` and the admin token exchange are private to this
//! crate; nothing outside it can name them, and
//! `scripts/check_architecture.py` fails the build if anything tries. That is
//! the same containment ADR 0001 applies to the NDC protocol in the runtime
//! plane, for the same reason: a representation that escapes its adapter turns
//! the platform's own model into a thin wrapper over somebody else's.
//!
//! # What this crate does not decide
//!
//! - **Whether to change anything.** It performs the actions the reconciler
//!   planned. It does not diff, and it holds no opinion about drift.
//! - **Whose authority it uses.** It is handed a bearer and presents it. That
//!   bearer belongs to the operator who asked for the change, because this
//!   platform holds no credential of its own for Keycloak (ADR 0012) — so
//!   there is nothing here for a deployment to deliver, and nothing to rotate.
//! - **What a realm should contain beyond the client contract.** It reconciles
//!   the realm, the required realm roles, and the declared application
//!   clients. Token lifespans, brute-force policy, themes and federation are
//!   left exactly as they are, deliberately.
//!
//! # It never deletes
//!
//! There is no path in this crate that removes a realm, a role, or a client.
//! The port it implements has no such operation; see `fabric-reconciliation`
//! for why.

mod admin;
mod config;
mod factory;
mod logging;
mod operator;
mod provider;
mod unavailable;
mod wire;

pub use config::KeycloakConfig;
pub use factory::KeycloakProviderFactory;
pub use operator::RealmSignIn;
pub use provider::KeycloakIdentityProvider;

/// The event-ID domain number for this crate. See `fabric_core::event_id`.
pub(crate) const DOMAIN_ID: u32 = 12;
