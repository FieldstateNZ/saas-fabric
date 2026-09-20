//! The one thing in this module that touches a port: composing a snapshot
//! from live platform state and offering it to a publication target, on
//! demand.

#[cfg(test)]
#[path = "publisher_tests.rs"]
mod publisher_tests;

use std::sync::atomic::Ordering;
use std::sync::Arc;

use fabric_core::Clock;
use fabric_runtime_publication::RuntimePublication;

use super::outcome::{PassOutcome, PassResult};
use super::state::PublicationState;
use crate::PlatformRepository;
use crate::RuntimeCatalogueSource;

/// Publishes one environment's runtime documents.
///
/// Reads what the platform has declared ([`crate::DataSourceState`]) and
/// recorded ([`crate::PlacementState`]), reads the derived catalogue
/// ([`RuntimeCatalogueSource`]), composes a complete
/// [`RuntimeSnapshot`](fabric_runtime_publication::RuntimeSnapshot), and
/// offers it to a [`RuntimePublication`] target -- a filesystem in tests, a
/// Kubernetes `ConfigMap` set in production. Everything this struct needs
/// is a port; nothing here knows whether the ports it is handed are backed
/// by Git, a fake, or the real cluster.
pub struct RuntimePublisher {
    pub(super) environment: String,
    pub(super) platform: Arc<dyn PlatformRepository>,
    pub(super) catalogue: Arc<dyn RuntimeCatalogueSource>,
    pub(super) target: Arc<dyn RuntimePublication>,
    pub(super) clock: Arc<dyn Clock>,
}

impl RuntimePublisher {
    /// Builds a publisher for one environment, over the ports it reads and
    /// the one it writes through.
    #[must_use]
    pub fn new(
        environment: String,
        platform: Arc<dyn PlatformRepository>,
        catalogue: Arc<dyn RuntimeCatalogueSource>,
        target: Arc<dyn RuntimePublication>,
        clock: Arc<dyn Clock>,
    ) -> Self {
        Self {
            environment,
            platform,
            catalogue,
            target,
            clock,
        }
    }

    /// A short description of the publication target, safe for logging and
    /// for the console's platform panel -- never a credential
    /// ([`RuntimePublication::describe`]'s own contract).
    #[must_use]
    pub fn describe_target(&self) -> String {
        self.target.describe()
    }

    /// Runs one publication pass, guarded against a second pass starting
    /// while this one is still in flight.
    ///
    /// The guard is released, and the pass recorded, on every path out of
    /// this method -- there is no early return that could leave the guard
    /// set, because the pass itself never propagates an error: every input
    /// failure becomes a [`PassOutcome`] value instead, the same shape
    /// [`crate::PlatformManagement::sweep`] uses for the same reason.
    pub async fn publish_once(&self, state: &PublicationState) -> PassResult {
        if state.running.swap(true, Ordering::SeqCst) {
            return PassResult::AlreadyRunning;
        }

        let outcome = self.run_pass().await;
        state.running.store(false, Ordering::SeqCst);

        self.log_pass(&outcome);
        state.record(self.clock.now_unix_seconds(), outcome.clone());

        PassResult::Ran(outcome)
    }

    /// Logs one structured event per pass -- `event =
    /// "control_plane.publication.pass"`, the target's own description, and
    /// the outcome. Every reason carried here is already a [`crate::SafeDiagnostic`],
    /// so nothing from a response body or a credential can reach a log
    /// line through this call.
    fn log_pass(&self, outcome: &PassOutcome) {
        let target = self.target.describe();

        match outcome {
            PassOutcome::Published { .. } | PassOutcome::Unchanged { .. } | PassOutcome::Waiting { .. } => {
                tracing::info!(
                    event = "control_plane.publication.pass",
                    environment = %self.environment,
                    target,
                    outcome = ?outcome,
                    "a publication pass completed"
                );
            }
            PassOutcome::Refused { reason } => {
                tracing::warn!(
                    event = "control_plane.publication.pass",
                    environment = %self.environment,
                    target,
                    reason = %reason,
                    "a publication pass was refused"
                );
            }
            PassOutcome::Failed { detail } => {
                tracing::warn!(
                    event = "control_plane.publication.pass",
                    environment = %self.environment,
                    target,
                    detail = %detail,
                    "a publication pass failed"
                );
            }
        }
    }
}
