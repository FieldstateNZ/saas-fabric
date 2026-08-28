//! Checking every connector at once, under a deadline.
//!
//! # Why this is not a `for` loop
//!
//! It used to be one, awaiting each connector in turn with no bound of its
//! own. The probe routes are merged *outside* the Data API's `TimeoutLayer` —
//! deliberately, since a probe that answers 504 is a probe the orchestrator
//! records as failed — so nothing capped it. Three connectors each stalling
//! 200ms made `/ready` take 607ms; at the connectors' own 10s default request
//! timeout, one blackholed backend would have held it open for ten seconds and
//! three for thirty.
//!
//! A kubelet `readinessProbe` defaults to `timeoutSeconds: 1`. Every one of
//! those numbers is a recorded probe failure and a replica pulled from
//! rotation — over connectors the readiness policy explicitly tolerates. The
//! decision was right; the I/O feeding it produced the opposite outcome.
//!
//! So the sweep runs every check concurrently and stops asking at
//! [`HEALTH_BUDGET`]. Whatever has not answered by then is
//! [`ConnectorHealth::Unknown`], and the checks still in flight are cancelled
//! when the [`JoinSet`] is dropped rather than left running behind the
//! response.

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use fabric_connector::{ConnectorRegistry, DataConnector};
use tokio::task::{Id, JoinSet};
use tokio::time::{timeout_at, Instant};

use crate::health::connector_health::{ConnectorHealth, ConnectorOutcome};

/// How long the whole sweep may take, however many connectors there are.
///
/// Two orders of magnitude below a connector's 10s default request timeout,
/// and comfortably inside a kubelet's 1s default probe budget with room to
/// spare for connection setup and scheduling. Deliberately not configurable:
/// this is not "how long may a health check take" — a connector's own timeout
/// answers that — it is "how long may this process take to give an
/// orchestrator an answer", which belongs to the probe contract rather than to
/// a deployment.
pub(super) const HEALTH_BUDGET: Duration = Duration::from_millis(500);

/// Checks every registered connector concurrently, giving up at
/// [`HEALTH_BUDGET`].
///
/// Outcomes come back in registry order, which is id order, so the probe body
/// is stable between calls and diffable by an operator.
pub(super) async fn sweep(connectors: &ConnectorRegistry) -> Vec<ConnectorOutcome> {
    let ids: Vec<String> = connectors
        .all()
        .map(|connector| connector.id().as_str().to_owned())
        .collect();

    let mut answers: Vec<Option<ConnectorHealth>> = Vec::new();
    answers.resize_with(ids.len(), || None);

    // Which slot each spawned check fills, keyed by task id. Keyed rather than
    // returned from the task because a task that panicked returns nothing at
    // all, and a connector with a buggy health check must still appear in the
    // body rather than silently shrinking the total the decision is taken over.
    let mut positions: HashMap<Id, usize> = HashMap::with_capacity(ids.len());
    let mut checks = JoinSet::new();

    for (position, connector) in connectors.all().enumerate() {
        let connector = Arc::clone(connector);
        let handle = checks.spawn(async move { check(&connector).await });
        positions.insert(handle.id(), position);
    }

    collect(&mut checks, &positions, &mut answers).await;

    ids.into_iter()
        .zip(answers)
        .map(|(id, health)| ConnectorOutcome {
            id,
            health: health.unwrap_or(ConnectorHealth::Unknown),
        })
        .collect()
}

/// Drains finished checks until the budget expires.
///
/// Anything still running is left as `None`, which [`sweep`] reads as
/// [`ConnectorHealth::Unknown`]. Returning early leaks no work: `sweep` drops
/// the [`JoinSet`], which aborts every task still in it.
async fn collect(
    checks: &mut JoinSet<ConnectorHealth>,
    positions: &HashMap<Id, usize>,
    answers: &mut [Option<ConnectorHealth>],
) {
    let deadline = Instant::now() + HEALTH_BUDGET;

    // `join_next_with_id` rather than `join_next`: the id is what maps an
    // answer back to the connector it came from, and without it a check that
    // finished second would be reported under the id of the one that finished
    // first.
    while let Ok(Some(joined)) = timeout_at(deadline, checks.join_next_with_id()).await {
        let (task, health) = match joined {
            Ok((task, health)) => (task, health),
            // A panicking health check is a bug in a connector implementation
            // rather than evidence about the backend. It is recorded as
            // unhealthy because the one thing it is definitely not is
            // serviceable.
            Err(error) => (
                error.id(),
                ConnectorHealth::Unhealthy("the connector health check panicked".to_owned()),
            ),
        };

        if let Some(slot) = positions.get(&task).and_then(|&at| answers.get_mut(at)) {
            *slot = Some(health);
        }
    }
}

/// Runs one connector's health check.
async fn check(connector: &Arc<dyn DataConnector>) -> ConnectorHealth {
    match connector.health().await {
        Ok(()) => ConnectorHealth::Healthy,
        Err(error) => ConnectorHealth::Unhealthy(error.to_string()),
    }
}
