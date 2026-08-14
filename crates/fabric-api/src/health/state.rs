//! What the probes look at.

use std::sync::Arc;

use fabric_connector::ConnectorRegistry;
use fabric_identity::IdentityResolver;
use fabric_tenant_runtime::RuntimeResolver;

/// The state the health endpoints read.
#[derive(Clone)]
pub struct HealthState {
    /// The runtime resolver, for the primed state *and contents* of both
    /// registries. Both halves matter — see the `readiness_state` submodule.
    pub runtime: Arc<RuntimeResolver>,

    /// The connectors, to check reachability.
    pub connectors: ConnectorRegistry,

    /// Resolves the caller's identity, to decide who may see the detail.
    ///
    /// The same resolver the Data API uses, shared rather than rebuilt: a
    /// probe that disagreed with the Data API about what a token means would
    /// be a second, quieter security decision.
    pub identity: Arc<IdentityResolver>,

    /// The role that may see the probe's detail.
    ///
    /// Taken from `ResourcePermissions::administrator_role`, the role that
    /// already carries estate-wide authority over every tenant's data. A
    /// deployment that trusts a role with that does not need a second,
    /// narrower credential invented for a diagnostic body — and inventing one
    /// would add a secret to distribute, rotate, and leak.
    pub administrator_role: String,
}
