//! A connector that records what it receives.
//!
//! A recording stub rather than a real database: what these tests check is
//! tenant resolution, scoping, and error mapping. A real database would add
//! setup cost without exercising any of it, and what *reaches* the connector is
//! exactly what the assertions are about.

use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use fabric_connector::{
    CollectionName, CollectionSchema, ConnectorCapabilities, ConnectorError, ConnectorId, ConnectorSchema,
    DataConnector, ExecutionTarget, MutationOutcome, MutationSpec, QueryOutcome, QuerySpec, Row,
};

use crate::support::field;

/// What the connector was asked to do.
#[derive(Default)]
struct Recorded {
    queries: Vec<(ExecutionTarget, QuerySpec)>,
    mutations: Vec<(ExecutionTarget, MutationSpec)>,
}

/// Records every operation and returns canned rows.
pub struct RecordingConnector {
    id: ConnectorId,
    capabilities: ConnectorCapabilities,
    schema: ConnectorSchema,
    recorded: Mutex<Recorded>,
    rows: Vec<Row>,
}

impl RecordingConnector {
    /// Builds a connector returning the given rows.
    pub fn new(rows: Vec<Row>) -> Arc<Self> {
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
            recorded: Mutex::new(Recorded::default()),
            rows,
        })
    }

    /// The most recent query, panicking if none arrived.
    pub fn last_query(&self) -> (ExecutionTarget, QuerySpec) {
        self.recorded
            .lock()
            .unwrap()
            .queries
            .last()
            .cloned()
            .expect("the connector received no query")
    }

    /// The most recent mutation, panicking if none arrived.
    pub fn last_mutation(&self) -> (ExecutionTarget, MutationSpec) {
        self.recorded
            .lock()
            .unwrap()
            .mutations
            .last()
            .cloned()
            .expect("the connector received no mutation")
    }

    /// How many queries reached the connector.
    pub fn query_count(&self) -> usize {
        self.recorded.lock().unwrap().queries.len()
    }

    /// How many mutations reached the connector.
    pub fn mutation_count(&self) -> usize {
        self.recorded.lock().unwrap().mutations.len()
    }
}

#[async_trait]
impl DataConnector for RecordingConnector {
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
        target: &ExecutionTarget,
        spec: &QuerySpec,
    ) -> Result<QueryOutcome, ConnectorError> {
        self.recorded
            .lock()
            .unwrap()
            .queries
            .push((target.clone(), spec.clone()));

        Ok(QueryOutcome::from_rows(self.rows.clone()))
    }

    /// Reports having applied everything it was handed.
    ///
    /// The count is derived from the spec rather than fixed at 1, because the
    /// Data API now checks a backend's count against the write it sent. A stub
    /// that claimed one row for a two-row insert would be modelling a
    /// *partially applied* batch — which is a real outcome, but one that
    /// belongs in the tests written for it (`write_outcome`), not in the
    /// default fixture every other suite builds on.
    async fn mutate(
        &self,
        target: &ExecutionTarget,
        spec: &MutationSpec,
    ) -> Result<MutationOutcome, ConnectorError> {
        self.recorded
            .lock()
            .unwrap()
            .mutations
            .push((target.clone(), spec.clone()));

        let affected = match spec {
            MutationSpec::Insert { rows, .. } => rows.len() as u64,
            // Both are keyed by the Data API, so they reach at most one record.
            MutationSpec::Update { .. } | MutationSpec::Delete { .. } => 1,
        };

        Ok(MutationOutcome::affected(affected))
    }

    async fn health(&self) -> Result<(), ConnectorError> {
        Ok(())
    }
}
