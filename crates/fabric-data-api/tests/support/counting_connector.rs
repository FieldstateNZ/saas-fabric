//! A connector that reports an affected-row count of the test's choosing.
//!
//! The other two stubs report counts that agree with what they were handed,
//! which is what every suite except one wants. This one exists to model the
//! backends that do *not*: a procedure that applied three of five rows, or one
//! whose result shape the connector misread into a count larger than the
//! request. Neither is reachable through `RecordingConnector`, and both are
//! exactly what `execution::write_integrity` is there to catch.

use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use fabric_connector::{
    CollectionName, CollectionSchema, ConnectorCapabilities, ConnectorError, ConnectorId, ConnectorSchema,
    DataConnector, ExecutionTarget, MutationOutcome, MutationSpec, QueryOutcome, QuerySpec, Row,
};

use crate::support::field;

/// Answers every write with a fixed affected-row count.
pub struct CountingConnector {
    id: ConnectorId,
    capabilities: ConnectorCapabilities,
    schema: ConnectorSchema,
    affected: u64,
    returns: Vec<Row>,
    mutations: Mutex<Vec<MutationSpec>>,
}

impl CountingConnector {
    /// Reports `affected` rows for every write, returning no rows.
    pub fn reporting(affected: u64) -> Arc<Self> {
        Arc::new(Self {
            affected,
            ..Self::base()
        })
    }

    /// Reports `affected` rows and hands back `returns`, as a backend
    /// implementing `RETURNING` does.
    pub fn reporting_with_rows(affected: u64, returns: Vec<Row>) -> Arc<Self> {
        Arc::new(Self {
            affected,
            returns,
            ..Self::base()
        })
    }

    /// How many writes reached the connector.
    pub fn mutation_count(&self) -> usize {
        self.mutations.lock().unwrap().len()
    }

    fn base() -> Self {
        Self {
            id: ConnectorId::try_new("postgres").unwrap(),
            capabilities: ConnectorCapabilities {
                mutations: true,
                ..ConnectorCapabilities::baseline()
            },
            schema: ConnectorSchema::new([(
                CollectionName::try_new("customers").unwrap(),
                CollectionSchema::new([field("id"), field("name"), field("salary"), field("tenant_key")]),
            )]),
            affected: 0,
            returns: Vec::new(),
            mutations: Mutex::new(Vec::new()),
        }
    }
}

#[async_trait]
impl DataConnector for CountingConnector {
    fn id(&self) -> &ConnectorId {
        &self.id
    }

    fn capabilities(&self) -> &ConnectorCapabilities {
        &self.capabilities
    }

    fn schema(&self) -> &ConnectorSchema {
        &self.schema
    }

    async fn query(&self, _: &ExecutionTarget, _: &QuerySpec) -> Result<QueryOutcome, ConnectorError> {
        Ok(QueryOutcome::from_rows(Vec::new()))
    }

    async fn mutate(
        &self,
        _: &ExecutionTarget,
        spec: &MutationSpec,
    ) -> Result<MutationOutcome, ConnectorError> {
        self.mutations.lock().unwrap().push(spec.clone());

        Ok(MutationOutcome::affected(self.affected).with_rows(self.returns.clone()))
    }

    async fn health(&self) -> Result<(), ConnectorError> {
        Ok(())
    }
}
