//! What the probes look at.

use std::sync::Arc;

use fabric_connector::ConnectorRegistry;
use fabric_identity::IdentityResolver;
use fabric_tenant_runtime::{ReaderStatus, RuntimeResolver};

/// The state the health endpoints read.
#[derive(Clone)]
pub struct HealthState {
    /// The runtime resolver, for the primed state *and contents* of both
    /// registries. Both halves matter — see the `readiness_state` submodule.
    pub runtime: Arc<RuntimeResolver>,

    /// Which generation the runtime serves, where from, and its
    /// last-known-good copy. A replica serving its copy is ready but
    /// degraded; one with neither a published generation nor a valid copy
    /// never primed, and the registries already make it unready.
    pub publication: Arc<ReaderStatus>,

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
    /// From `AppConfig::health_detail_role`. A coarse operator control kept
    /// within its existing boundary (D01-12): it opens this body and grants
    /// nothing on the data path. Reusing a role the operator's token already
    /// carries avoids inventing a credential to distribute, rotate and leak
    /// for a diagnostic body.
    pub administrator_role: String,
}
