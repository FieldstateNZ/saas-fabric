//! A connector whose calls sleep, so a test can prove a dropped request
//! cancels the in-flight work rather than letting it run to completion in
//! the background.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use fabric_connector::{
    CollectionName, CollectionSchema, ConnectorCapabilities, ConnectorError, ConnectorId, ConnectorSchema,
    DataConnector, ExecutionTarget, MutationOutcome, MutationSpec, QueryOutcome, QuerySpec,
};

use crate::support::field;

/// A connector that sleeps before answering, recording whether it was ever
/// polled to completion.
///
/// `started` flips as soon as the call begins, before the sleep — proving the
/// request genuinely reached the connector. `finished` only flips after the
/// sleep, so a test that drops the request early and later finds `finished`
/// still false has proven the future was cancelled, not merely that it was
/// slow to observe.
pub struct DelayedConnector {
    id: ConnectorId,
    capabilities: ConnectorCapabilities,
    schema: ConnectorSchema,
    delay: Duration,
    started: Arc<AtomicBool>,
    finished: Arc<AtomicBool>,
}

impl DelayedConnector {
    /// Builds a connector whose `query` and `mutate` calls sleep for `delay`
    /// before returning an empty, successful outcome.
    pub fn new(delay: Duration) -> Arc<Self> {
        let schema = ConnectorSchema::new([(
            CollectionName::try_new("customers").unwrap(),
            CollectionSchema::new([field("id"), field("name"), field("tenant_key")]),
        )]);

        Arc::new(Self {
            id: ConnectorId::try_new("postgres").unwrap(),
            capabilities: ConnectorCapabilities {
                mutations: true,
                ..ConnectorCapabilities::baseline()
            },
            schema,
            delay,
            started: Arc::new(AtomicBool::new(false)),
            finished: Arc::new(AtomicBool::new(false)),
        })
    }

    /// Whether a call has begun.
    pub fn started(&self) -> bool {
        self.started.load(Ordering::SeqCst)
    }

    /// Whether a call has run to completion.
    pub fn finished(&self) -> bool {
        self.finished.load(Ordering::SeqCst)
    }
}

#[async_trait]
impl DataConnector for DelayedConnector {
    fn id(&self) -> &ConnectorId {
        &self.id
    }

    fn capabilities(&self) -> &ConnectorCapabilities {
        &self.capabilities
    }

    fn schema(&self) -> &ConnectorSchema {
        &self.schema
    }

    async fn query(
        &self,
        _target: &ExecutionTarget,
        _spec: &QuerySpec,
    ) -> Result<QueryOutcome, ConnectorError> {
        self.started.store(true, Ordering::SeqCst);
        tokio::time::sleep(self.delay).await;
        self.finished.store(true, Ordering::SeqCst);

        Ok(QueryOutcome::default())
    }

    async fn mutate(
        &self,
        _target: &ExecutionTarget,
        _spec: &MutationSpec,
    ) -> Result<MutationOutcome, ConnectorError> {
        self.started.store(true, Ordering::SeqCst);
        tokio::time::sleep(self.delay).await;
        self.finished.store(true, Ordering::SeqCst);

        Ok(MutationOutcome::affected(1))
    }

    async fn health(&self) -> Result<(), ConnectorError> {
        Ok(())
    }
}
