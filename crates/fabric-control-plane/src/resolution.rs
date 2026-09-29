//! Resolving a selected component version against the registries (ADR 0026
//! section 7).
//!
//! # The second operator request whose write depends on a registry read
//!
//! [`ClientService`](crate::ClientService) calls no platform service, and
//! still does not: an operator's selection is resolved here, by a service of
//! its own, handed the registry port the composition root already builds and
//! the registry service that says which repositories are registered. The
//! handler reads the catalogue first — its revision, the application, the
//! component's kind — then asks this service, then hands the resolution to
//! the catalogue's pure `select_component` for the write. Rollback is the
//! first request whose write depends on a registry read; this is the second,
//! and the control-plane architecture names it as the exception it is.
//!
//! # Bounded, and safe to cut off
//!
//! A resolution runs against a deadline, the deployment's
//! `[registries].resolution_budget_seconds`. When it is reached the future
//! asking the registry is dropped, which drops every read still in flight:
//! reads are safe to abandon, and nothing is written until the resolution
//! is whole. Startup refuses a configuration where a Git read, the budget
//! and a Git write do not fit under one request.

mod refusal;
mod resolve;
mod selectable;

use std::sync::Arc;
use std::time::Duration;

use fabric_core::Clock;
use fabric_platform_management::Registry;

pub use refusal::{SelectionRefusal, Unusable};
pub(crate) use selectable::selectable;

use crate::RegistryService;

/// What the composition root hands in for resolving selections.
///
/// A struct rather than two more fields on the dependencies, so the port and
/// the budget that bounds reading through it arrive together.
pub struct ResolutionParts {
    /// Where images and component descriptors are read: the router every
    /// operator-registered registry is installed into, the one Platform
    /// Management reads through.
    pub registry: Arc<dyn Registry>,

    /// How long one resolution may take, every registry read included.
    pub budget: Duration,
}

/// Resolves a repository and a version tag into what the catalogue records.
pub(crate) struct ResolutionService {
    /// Where images and component descriptors are read.
    registry: Arc<dyn Registry>,

    /// Which repositories are registered.
    registries: Arc<RegistryService>,

    /// How long one resolution may take.
    budget: Duration,

    /// Stamps when a version was resolved.
    clock: Arc<dyn Clock>,
}

impl ResolutionService {
    /// The service over `parts`, holding selections to what `registries`
    /// has registered.
    pub(crate) fn new(
        parts: ResolutionParts,
        registries: Arc<RegistryService>,
        clock: Arc<dyn Clock>,
    ) -> Self {
        Self {
            registry: parts.registry,
            registries,
            budget: parts.budget,
            clock,
        }
    }
}
