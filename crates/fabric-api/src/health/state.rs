//! What the probes look at.

use std::sync::Arc;

use fabric_connector::ConnectorRegistry;
use fabric_tenant_runtime::RuntimeResolver;

/// The state the health endpoints read.
#[derive(Clone)]
pub struct HealthState {
    /// The runtime resolver, for the primed state of both registries.
    pub runtime: Arc<RuntimeResolver>,

    /// The connectors, to check reachability.
    pub connectors: ConnectorRegistry,
}
