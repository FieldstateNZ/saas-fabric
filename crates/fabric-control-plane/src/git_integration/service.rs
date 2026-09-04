//! Connecting this platform to where client desired state lives.
//!
//! # The order matters, and each step is only recorded once it is proven
//!
//! 1. An operator names the organisation. The platform describes the
//!    application it wants and hands the browser to the host.
//! 2. The host returns a one-time code. The platform redeems it, **stores the
//!    private key first**, and only then records the application — a record
//!    without its key is an integration that can never mint anything, and the
//!    key arrives exactly once.
//! 3. The operator installs the application. The platform **mints a token
//!    before recording the installation**, so a recorded installation always
//!    means a working one.
//! 4. If the installation reaches exactly one repository, that is the one. If
//!    it reaches several, the platform declines to guess and the operator
//!    chooses.

mod binding;
mod candidates;
mod connect;
mod disconnect;
mod errors;
mod install;
mod restore;
mod settling;
mod stored;

use std::sync::Arc;

use fabric_core::Clock;

use crate::git_integration::{
    GitAppProvisioning, IntegrationKind, IntegrationStore, IntegrationTarget, PendingFlows, SecretStore,
};

pub use errors::IntegrationError;

/// Everything the connection flow needs.
pub struct GitIntegrationService {
    /// Which integration this instance is for.
    ///
    /// One service per integration, and it carries its own kind rather than
    /// taking one per call. A caller that could pass a kind could pass the
    /// wrong one, and the wrong one here means connecting client
    /// configuration and disconnecting platform management.
    kind: IntegrationKind,

    /// Creates and inspects the application on the host.
    provisioning: Arc<dyn GitAppProvisioning>,

    /// Where the private key goes.
    secrets: Arc<dyn SecretStore>,

    /// Where the record goes.
    store: Arc<dyn IntegrationStore>,

    /// Flows started and not yet completed.
    flows: Arc<PendingFlows>,

    /// What this integration points at once it is usable.
    target: Arc<dyn IntegrationTarget>,

    /// Stamps flow expiry.
    clock: Arc<dyn Clock>,

    /// One transition at a time.
    ///
    /// A transition records what the operator settled on and points the live
    /// binding at it — one change written to two places. Two overlapping could
    /// interleave into a record naming one repository and a binding pointing
    /// at another; held across the whole of each, the platform ends on
    /// whichever ran last instead of half of each. See `settling.rs`.
    transitions: Arc<tokio::sync::Mutex<()>>,
}

impl GitIntegrationService {
    /// Assembles the service.
    #[must_use]
    pub fn new(
        kind: IntegrationKind,
        provisioning: Arc<dyn GitAppProvisioning>,
        secrets: Arc<dyn SecretStore>,
        store: Arc<dyn IntegrationStore>,
        target: Arc<dyn IntegrationTarget>,
        clock: Arc<dyn Clock>,
    ) -> Self {
        Self {
            kind,
            provisioning,
            secrets,
            store,
            flows: Arc::new(PendingFlows::new()),
            target,
            clock,
            transitions: Arc::new(tokio::sync::Mutex::new(())),
        }
    }
}
