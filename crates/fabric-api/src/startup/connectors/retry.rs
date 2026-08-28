//! Retrying connectors that failed startup negotiation, in the background.

mod retry_handle;
#[cfg(test)]
mod retry_tests;

use std::sync::Arc;
use std::time::Duration;

use fabric_connector::SecretResolver;
use fabric_connector_ndc::{build_ndc_connector, NdcConnectorConfig};
use tokio::sync::Notify;

pub use retry_handle::ConnectorRetryHandle;

use crate::startup::connectors::logging;
use crate::startup::connectors::pending_connector::PendingConnector;

/// A connector still waiting to be negotiated, and what retrying it needs.
pub(super) struct PendingRetry {
    config: NdcConnectorConfig,
    placeholder: Arc<PendingConnector>,
}

impl PendingRetry {
    /// Pairs a failed connector's configuration with the placeholder that
    /// stands in for it until it negotiates successfully.
    pub(super) const fn new(config: NdcConnectorConfig, placeholder: Arc<PendingConnector>) -> Self {
        Self { config, placeholder }
    }
}

/// Starts the background retry loop (§35).
///
/// Ticks on `interval`, attempting every connector still in `pending`. One
/// that succeeds is removed from the list: its capabilities and schema are
/// now cached inside the real connector installed behind the placeholder,
/// there is nothing left to retry, and the readiness probe already checks it
/// live from then on through its own `health()`. One that fails again stays
/// in the list for the next tick, with its recorded reason updated.
///
/// Exits, without ever ticking, once told to shut down — or once `pending`
/// starts empty, which is the common case where every configured connector
/// negotiated at startup and there is nothing to watch.
#[must_use]
pub(super) fn spawn(
    pending: Vec<PendingRetry>,
    secrets: Arc<dyn SecretResolver>,
    interval: Duration,
) -> ConnectorRetryHandle {
    let shutdown = Arc::new(Notify::new());
    let task_shutdown = Arc::clone(&shutdown);

    let task = tokio::spawn(async move {
        let mut pending = pending;

        loop {
            if pending.is_empty() {
                task_shutdown.notified().await;
                break;
            }

            tokio::select! {
                () = tokio::time::sleep(interval) => {}
                () = task_shutdown.notified() => break,
            }

            pending = retry_once(pending, &secrets).await;
        }
    });

    ConnectorRetryHandle::new(shutdown, task)
}

/// Attempts every still-failed connector once, returning the ones still
/// failing after this attempt.
async fn retry_once(pending: Vec<PendingRetry>, secrets: &Arc<dyn SecretResolver>) -> Vec<PendingRetry> {
    let mut still_pending = Vec::new();

    for retry in pending {
        match build_ndc_connector(retry.config.clone(), Some(Arc::clone(secrets))).await {
            Ok(connector) => {
                logging::connector_recovered(retry.config.id.as_str());
                retry.placeholder.resolve(connector);
            }
            Err(reason) => {
                logging::retry_failed(retry.config.id.as_str(), &reason);
                retry.placeholder.record_failure(reason);
                still_pending.push(retry);
            }
        }
    }

    still_pending
}
