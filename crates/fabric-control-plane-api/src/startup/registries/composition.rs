//! What the registries are composed from, and what composing them gives.

use std::sync::Arc;
use std::time::Duration;

use fabric_control_plane::{RegistryService, RegistryStore, ResolutionParts, SecretStore};
use fabric_core::Clock;
use fabric_platform_management::Registry;
use fabric_registry::Registries;

use crate::config::RegistryBinding;

/// What the registries are composed from.
pub struct RegistryComposition {
    /// The deployment's own registry, when Platform Management is
    /// configured.
    pub deployment: Option<RegistryBinding>,

    /// `[registries].http_timeout_seconds`, for every registry an operator
    /// registers.
    pub http_timeout_seconds: u64,

    /// `[registries].resolution_budget_seconds`: how long resolving one
    /// selected component version may take.
    pub resolution_budget_seconds: u64,

    /// Where the record set is kept.
    pub store: Arc<dyn RegistryStore>,

    /// Where each credential's token is kept.
    pub secrets: Arc<dyn SecretStore>,

    /// Stamps records.
    pub clock: Arc<dyn Clock>,
}

/// The composed registries.
pub struct ComposedRegistries {
    /// What operators register registries with.
    pub service: Arc<RegistryService>,

    /// What Platform Management reads through.
    pub router: Arc<Registries>,

    /// How long resolving one selected component version may take.
    pub resolution_budget: Duration,
}

impl ComposedRegistries {
    /// What a selected component version is resolved through: the router
    /// Platform Management reads through, so a selection and discovery read
    /// every registry the same way, and the budget one resolution may take.
    #[must_use]
    pub fn resolution(&self) -> ResolutionParts {
        ResolutionParts {
            registry: Arc::clone(&self.router) as Arc<dyn Registry>,
            budget: self.resolution_budget,
        }
    }
}
