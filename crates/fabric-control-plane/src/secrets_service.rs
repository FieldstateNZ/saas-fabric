//! Managing a client's secrets, with the boundary resolved rather than
//! supplied.
//!
//! # The resolution is the security property
//!
//! ```text
//! operator names a client and a path
//!         ↓
//! the client's desired state
//!         ↓
//! its declared boundary
//!         ↓
//! the store, inside that boundary
//! ```
//!
//! A caller never names a boundary. A client with none declared is refused
//! rather than guessed at, because a guessed boundary is another client's
//! boundary sooner or later.
//!
//! # Every operation is recorded, including reading
//!
//! Reveal is an act, not a side effect of looking at a page, and it is the act
//! an investigation most wants to see. The record carries the actor, the
//! client, the path and the outcome, and never a value or a key name.

mod operations;

use std::sync::Arc;

use fabric_client_model::ClientId;

use crate::{ClientSecrets, ClientService, ControlPlaneError, SecretsError};

/// One client's secrets, addressed by client rather than by boundary.
pub struct SecretsService {
    /// Where a client's declared boundary is read from.
    clients: Arc<ClientService>,

    /// The store those secrets live in.
    store: Arc<dyn ClientSecrets>,
}

impl SecretsService {
    /// Builds the service.
    #[must_use]
    pub const fn new(clients: Arc<ClientService>, store: Arc<dyn ClientSecrets>) -> Self {
        Self { clients, store }
    }

    /// The boundary a client declared, or a refusal.
    async fn boundary(
        &self,
        client: &ClientId,
    ) -> Result<fabric_client_model::SecretNamespace, ControlPlaneError> {
        let stored = self.clients.get(client).await?;

        stored
            .document
            .client()
            .secrets
            .as_ref()
            .map(|secrets| secrets.namespace.clone())
            .ok_or(ControlPlaneError::Secrets(SecretsError::NoBoundary))
    }
}

/// How an operation turned out, for the record.
///
/// `pub(super)` so the operations beside this file share one spelling of each
/// outcome; an audit trail whose vocabulary drifts is one nobody can query.
///
/// A refusal is recorded as loudly as a success: an attempt that failed is
/// often the more interesting half of an investigation.
pub(super) fn outcome<T>(result: &Result<T, SecretsError>) -> &'static str {
    match result {
        Ok(_) => "succeeded",
        Err(SecretsError::Conflict) => "refused_stale_version",
        Err(SecretsError::NotFound) => "not_found",
        Err(SecretsError::NoBoundary) => "no_boundary",
        Err(SecretsError::Refused) => "refused_by_store",
        Err(SecretsError::Unavailable) => "store_unavailable",
    }
}
