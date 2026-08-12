//! A connector that failed startup negotiation, retried in the background.
//!
//! The shared state and the two writes that mutate it live here. The
//! [`DataConnector`] implementation that reads it on every operation is next
//! door in `pending_connector_delegation.rs`, because those are two different
//! jobs: this file is about what a pending connector *is*, that one is about
//! what happens when someone tries to use it.

use std::collections::BTreeSet;
use std::sync::{Arc, PoisonError, RwLock};

use fabric_connector::{ConnectorCapabilities, ConnectorError, ConnectorId, ConnectorSchema, DataConnector};

use crate::startup::connectors::negotiation_failure::NegotiationFailure;

/// A connector registered under its id even though negotiation has not (yet)
/// succeeded.
///
/// # Why this exists
///
/// [`ConnectorRegistry`](fabric_connector::ConnectorRegistry) is a value
/// cloned into every place that needs connector access — the readiness probe,
/// the Data API. Once those clones exist there is no reaching back in to
/// replace one entry; the registry's only mutation is building a new one. So
/// instead of leaving a failed connector's id unregistered — which would make
/// a request naming it indistinguishable from a tenant binding naming a
/// connector nobody configured at all, i.e.
/// [`ConnectorError::UnknownConnector`] — this type is registered *in its
/// place*. Every clone of the registry shares the same `Arc<PendingConnector>`,
/// so when the background retry loop calls [`Self::resolve`], every holder
/// sees the negotiated connector on its very next lookup. No restart, no
/// re-cloning (§35).
///
/// # Why a lock, not lock-free
///
/// This is exactly the "genuine shared-state seam" a lock is for: one
/// background task occasionally *writes* a resolved connector or a failure
/// reason, many request-handling tasks occasionally *read* one. `std::sync`
/// rather than `tokio::sync`, because the critical section is a pointer clone
/// with no `.await` inside it — see [`Self::resolved_connector`], which
/// clones the `Arc` out and drops the guard before any lock holder crosses an
/// await point (holding a `std::sync` guard across `.await` would not even
/// compile under a multi-threaded runtime, since the guard is not `Send`).
///
/// # `capabilities()` and `schema()` are always empty
///
/// Nothing outside a connector's own `query`/`mutate` implementation reads
/// these two accessors in this codebase today — capability checks happen
/// inside the concrete connector, against its own cached fields, not through
/// the trait. So an empty, "supports nothing" answer here is both safe (the
/// honest state before negotiation) and, in practice, never consulted. It
/// does **not** update after [`Self::resolve`]: the real answer lives on the
/// connector now installed behind `query`/`mutate`/`health`, which is where
/// every current caller goes.
pub(super) struct PendingConnector {
    id: ConnectorId,
    resolved: RwLock<Option<Arc<dyn DataConnector>>>,
    reason: RwLock<String>,
    capabilities: ConnectorCapabilities,
    schema: ConnectorSchema,
}

impl PendingConnector {
    /// Registers a connector that failed startup negotiation.
    ///
    /// `reason` is the negotiation failure, exactly as it will be reported by
    /// `/ready` and by any request routed here until [`Self::resolve`] is
    /// called.
    #[must_use]
    pub(super) fn new(id: ConnectorId, reason: String) -> Arc<Self> {
        Arc::new(Self {
            id,
            resolved: RwLock::new(None),
            reason: RwLock::new(reason),
            capabilities: ConnectorCapabilities {
                filtering: false,
                ordering: false,
                paging: false,
                mutations: false,
                transactional_mutations: false,
                total_count: false,
                comparisons: BTreeSet::new(),
            },
            schema: ConnectorSchema::default(),
        })
    }

    /// Installs a negotiated connector.
    ///
    /// Every clone of the registry this was registered into shares this same
    /// `Arc`, so the update is visible to every caller's next lookup.
    pub(super) fn resolve(&self, connector: Arc<dyn DataConnector>) {
        *self.resolved.write().unwrap_or_else(PoisonError::into_inner) = Some(connector);
    }

    /// Records why the most recent retry attempt failed, so `/ready` and the
    /// next operation report the current reason rather than the one from
    /// startup.
    pub(super) fn record_failure(&self, reason: String) {
        *self.reason.write().unwrap_or_else(PoisonError::into_inner) = reason;
    }

    /// This connector's id, as configured.
    pub(super) const fn id(&self) -> &ConnectorId {
        &self.id
    }

    /// Always the empty, supports-nothing set — see the type-level docs.
    pub(super) const fn capabilities(&self) -> &ConnectorCapabilities {
        &self.capabilities
    }

    /// Always empty — see the type-level docs.
    pub(super) const fn schema(&self) -> &ConnectorSchema {
        &self.schema
    }

    /// Clones out the resolved connector, if any, and releases the lock
    /// immediately — never held across an `.await`.
    pub(super) fn resolved_connector(&self) -> Option<Arc<dyn DataConnector>> {
        self.resolved
            .read()
            .unwrap_or_else(PoisonError::into_inner)
            .clone()
    }

    /// The error every operation returns before negotiation succeeds.
    pub(super) fn unavailable(&self) -> ConnectorError {
        let reason = self.reason.read().unwrap_or_else(PoisonError::into_inner).clone();

        ConnectorError::Unreachable {
            connector: self.id.clone(),
            source: Box::new(NegotiationFailure(reason)),
        }
    }
}
