//! The one thing in this module that touches a port: composing a snapshot
//! from live platform state and offering it to a publication target, on
//! demand.
//!
//! In the 121-150 band: `RuntimePublisher`, its constructors and
//! `publish_once`, with `log_pass` -- the one place `PassOutcome`'s match
//! must stay exhaustive against every variant `publish_once` produces.

#[cfg(test)]
#[path = "publisher_tests.rs"]
mod publisher_tests;

use std::sync::{Arc, Mutex};

use fabric_core::Clock;
use fabric_runtime_publication::RuntimePublication;

use super::issuer_registry_source::IssuerRegistrySource;
use super::outcome::{PassOutcome, PassResult};
use super::published::Accepted;
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
/// Kubernetes `ConfigMap` set in production. Everything it needs is a port.
pub struct RuntimePublisher {
    pub(super) environment: String,
    pub(super) platform: Arc<dyn PlatformRepository>,
    pub(super) catalogue: Arc<dyn RuntimeCatalogueSource>,
    pub(super) target: Arc<dyn RuntimePublication>,
    pub(super) clock: Arc<dyn Clock>,
    /// Where the issuer registry comes from; `None` publishes none (D01-4).
    pub(super) registry: Option<Arc<dyn IssuerRegistrySource>>,
    /// What the target accepted in the pass running now, handed to
    /// [`PublicationState`] when it ends. Only one pass runs at a time, so
    /// this is never shared between two.
    pub(super) accepted: Mutex<Option<Accepted>>,
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
            registry: None,
            accepted: Mutex::new(None),
        }
    }

    /// Also publishes the issuer registry `source` decides, with every pass.
    #[must_use]
    pub fn with_issuer_registry(self, source: Arc<dyn IssuerRegistrySource>) -> Self {
        Self {
            registry: Some(source),
            ..self
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
    /// The guard is released by its own `Drop`, never by code after the
    /// `.await`, which a panic or a dropped future would skip -- see
    /// `RunningGuard`'s rustdoc (`running_guard.rs`).
    pub async fn publish_once(&self, state: &PublicationState) -> PassResult {
        let Some(_guard) = state.running.try_enter() else {
            return PassResult::AlreadyRunning;
        };

        let outcome = self.run_pass().await;
        let accepted = self.take_accepted();

        self.log_pass(&outcome);
        let at_unix_seconds = self.clock.now_unix_seconds();
        state.record(at_unix_seconds, outcome.clone(), accepted);

        PassResult::Ran {
            at_unix_seconds,
            outcome,
        }
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
