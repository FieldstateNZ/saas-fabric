//! The platform's own Git integration, as durable state rather than config.
//!
//! # Why this is a domain concept and not a deployment file
//!
//! Everything here used to live in a TOML file that a human filled in after
//! hand-creating a GitHub App and copying its private key. That made the
//! platform's onboarding somebody else's job and made the credential a thing
//! people handled. The App is now created *by* the platform, and what it
//! learns in the process is state the platform owns.
//!
//! # Instance-scoped, deliberately
//!
//! Only the master instance needs an integration today. The ports are scoped
//! to *an* instance rather than assuming *the* instance — a store is built for
//! one and knows nothing of others — so a per-client integration later is a
//! second store rather than a rewrite of this module.

mod flow;
#[cfg(test)]
mod flow_tests;
mod in_memory;
mod kind;
mod provisioning;
mod record;
#[cfg(test)]
mod record_tests;
mod secret_store;
mod service;
#[cfg(test)]
mod service_tests;
mod store;

pub use flow::{FlowStep, PendingFlows};
pub use in_memory::{InMemoryIntegrationStore, InMemorySecretStore};
pub use kind::IntegrationKind;
pub use provisioning::{
    AccessibleRepository, AppCreationRequest, CreatedApp, DesiredStateFactory, GitAppProvisioning,
    InstallationDetail, ProvisioningError,
};
pub use record::{GitIntegration, Installation, SelectedRepository};
pub use secret_store::{SecretName, SecretStore, SecretStoreError, SecretValue};
pub use service::{GitIntegrationService, IntegrationError};
pub use store::{IntegrationStore, IntegrationStoreError};
