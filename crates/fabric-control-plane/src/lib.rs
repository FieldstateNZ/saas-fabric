//! The SaaS Fabric control plane: where an operator expresses what a client
//! should have.
//!
//! ```text
//! operator                     a human, on the operator network
//!     ↓
//! control-plane UI             apps/control-plane-ui
//!     ↓
//! Control Plane API            this crate
//!     ↓
//! ClientRepository             this crate's port
//!     ↓
//! Git desired state            fabric-client-git → saas-fabric-clients
//!     ↓
//! reconciliation               fabric-reconciliation
//!     ↓
//! Keycloak                     fabric-keycloak
//! ```
//!
//! # The rule this crate exists to enforce
//!
//! **An operator mutation writes desired state. It does not call a platform
//! service.** There is no code path from a handler to Keycloak. Identity is
//! changed by writing a document to Git and letting reconciliation converge
//! the provider onto it, which is what makes Git the authority rather than one
//! of two competing ones (ADR 0008).
//!
//! The visible consequence: a successful `PUT` answers `pending`, not
//! `applied`. Writing the document and converging the provider are different
//! events that fail independently, and an API that reported them as one would
//! be lying about the second one every time.
//!
//! # Two identities that must never meet
//!
//! The runtime plane resolves a **tenant** from a bearer token and serves that
//! tenant's data. This crate authenticates a **platform operator** and lets
//! them administer any client. Nothing here reads a `tenant_id`, and no
//! operator authority is derived from one — see [`OperatorAuthenticator`] and
//! ADR 0009.
//!
//! # What is deliberately absent
//!
//! - **Client creation.** `POST /api/clients` is not implemented: creating a
//!   client is a workflow (routing, data placement, secrets, a database) and
//!   this increment is the identity slice of it.
//! - **Deletion of anything.** Neither documents nor realm content.
//! - **Raw document editing.** The API exposes realms, roles and application
//!   clients. It never exposes a file path, a line number, or YAML.

mod audit;
mod config;
mod converge;
mod errors;
mod extraction;
#[cfg(test)]
mod fixtures;
mod git_integration;
mod handlers;
mod identity_authority;
mod integration;
mod logging;
mod models;
mod operator;
mod preconditions;
mod reconcile;
mod registration;
mod repository;
mod routes;
mod service;
mod sign_in;
mod state;
pub mod testing;

pub use config::{ControlPlaneConfig, OperatorConfig, ReconciliationConfig};
pub use errors::ControlPlaneError;
pub use git_integration::{
    AccessibleRepository, AppCreationRequest, CreatedApp, DesiredStateFactory, GitAppProvisioning,
    GitIntegration, GitIntegrationService, InMemoryIntegrationStore, InMemorySecretStore, Installation,
    InstallationDetail, IntegrationError, IntegrationStore, IntegrationStoreError, PendingFlows,
    ProvisioningError, SecretName, SecretStore, SecretStoreError, SecretValue, SelectedRepository,
};
pub use identity_authority::IdentityProviderFactory;
pub use integration::{IntegrationHealth, IntegrationStatus, Observation};
pub use operator::{
    KeyHolder, OidcOperators, Operator, OperatorAuthError, OperatorAuthenticator, OperatorToken,
    VerificationKeys,
};
pub use registration::{build_control_plane, ControlPlaneDeps, ControlPlaneServices};
pub use repository::{
    ChangeContext, ClientRepository, DesiredStateBinding, InMemoryClientRepository, RepositoryError,
    StoredClient,
};
pub use routes::API_PREFIX;
pub use service::ClientService;
pub use sign_in::{IssuedToken, OperatorSignIn, SignInError, SignInSurface};

/// The event-ID domain number for this crate. See `fabric_core::event_id`.
pub(crate) const DOMAIN_ID: u32 = 10;
