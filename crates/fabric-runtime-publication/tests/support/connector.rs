//! A connector that behaves like the one shared table ADR 0018 describes:
//! every tenant's rows sit together in one corpus, and the only thing that
//! keeps them apart is whatever predicate the request actually carried.
//!
//! This is deliberately not a stub that returns canned rows keyed by which
//! tenant is asking. A connector that ignored the predicate and dispatched on
//! the tenant identity instead would make every isolation assertion in this
//! suite pass whether or not the platform ever built a predicate at all --
//! exactly the kind of test `docs/delivery.md` warns cannot fail. Applying
//! the predicate to a shared corpus is what makes the isolation tests
//! sensitive to the thing they exist to prove.

use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use fabric_connector::{
    CollectionName, CollectionSchema, ComparisonOperator, ConnectorCapabilities, ConnectorError, ConnectorId,
    ConnectorSchema, DataConnector, ExecutionTarget, Filter, MutationOutcome, MutationSpec, QueryOutcome,
    QuerySpec, Row,
};
use serde_json::Value;

use crate::support::field;
use crate::support::fixtures::CONNECTOR_ID;

/// Two rows under the same logical key (`id: "1"`), one per tenant --
/// exactly what a shared table with a discriminator column looks like on
/// disk, and the shape that proves isolation happens at the predicate rather
/// than by coincidence of which rows exist.
fn corpus() -> Vec<Row> {
    vec![
        Row::new()
            .with(field("id"), Value::String("1".to_owned()))
            .with(field("title"), Value::String("Acme Handbook".to_owned()))
            .with(
                field("tenant_key"),
                // Literal, not the fixture's constant: the corpus is the database's own
                // truth, and it must not move when a binding is mutated -- otherwise
                // zeroing the published discriminator moves both sides together and
                // the isolation tests stay green over a broken binding.
                Value::String("tenant-acme-482".to_owned()),
            ),
        Row::new()
            .with(field("id"), Value::String("1".to_owned()))
            .with(field("title"), Value::String("Globex Playbook".to_owned()))
            .with(field("tenant_key"), Value::String("tenant-globex-915".to_owned())),
    ]
}

/// Whether `row` matches `filter`. Understands exactly the shapes this
/// fixture's requests produce: an equality compare, and the conjunctions
/// `QuerySpec::for_target` and the key-read path build out of a caller
/// filter and the tenant predicate.
fn row_matches(row: &Row, filter: &Filter) -> bool {
    match filter {
        Filter::And { clauses } => clauses.iter().all(|clause| row_matches(row, clause)),
        Filter::Or { clauses } => clauses.iter().any(|clause| row_matches(row, clause)),
        Filter::Not { clause } => !row_matches(row, clause),
        Filter::Compare {
            field,
            operator,
            value,
        } => {
            let actual = row.get(field);
            match operator {
                ComparisonOperator::Equal => actual == Some(value),
                ComparisonOperator::NotEqual => actual != Some(value),
                other => panic!("this fixture's connector only speaks equality; got {other:?} on {field:?}"),
            }
        }
        Filter::IsNull { field } => row.get(field).is_none(),
        Filter::In { field, values } => row.get(field).is_some_and(|actual| values.contains(actual)),
    }
}

/// What one call handed the connector.
type Captured = (ExecutionTarget, QuerySpec);

/// Records every query it receives -- target and predicate both -- and
/// answers by applying the predicate to a small in-memory corpus, the way a
/// real shared table would.
pub struct RecordingConnector {
    id: ConnectorId,
    capabilities: ConnectorCapabilities,
    schema: ConnectorSchema,
    rows: Vec<Row>,
    queries: Mutex<Vec<Captured>>,
}

impl RecordingConnector {
    /// Builds the connector over the fixture's two-row corpus.
    #[must_use]
    pub fn new() -> Arc<Self> {
        let schema = ConnectorSchema::new([(
            CollectionName::try_new("articles").unwrap(),
            CollectionSchema::new([field("id"), field("title"), field("tenant_key")]),
        )]);

        Arc::new(Self {
            id: ConnectorId::try_new(CONNECTOR_ID).unwrap(),
            capabilities: ConnectorCapabilities::baseline(),
            schema,
            rows: corpus(),
            queries: Mutex::new(Vec::new()),
        })
    }

    /// The most recent query, panicking if none arrived.
    pub fn last_query(&self) -> Captured {
        self.queries
            .lock()
            .unwrap()
            .last()
            .cloned()
            .expect("the connector received no query")
    }

    /// How many queries reached the connector.
    pub fn query_count(&self) -> usize {
        self.queries.lock().unwrap().len()
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
        self.queries.lock().unwrap().push((target.clone(), spec.clone()));

        let matched: Vec<Row> = match &spec.filter {
            Some(filter) => self
                .rows
                .iter()
                .filter(|row| row_matches(row, filter))
                .cloned()
                .collect(),
            // No predicate at all: a real shared table would hand back every
            // tenant's rows, which is the leak this fixture exists to be
            // able to reproduce if the guard it stands in for ever regresses.
            None => self.rows.clone(),
        };

        Ok(QueryOutcome::from_rows(matched))
    }

    async fn mutate(
        &self,
        _target: &ExecutionTarget,
        _spec: &MutationSpec,
    ) -> Result<MutationOutcome, ConnectorError> {
        panic!("this fixture's catalogue is read-only; no test in this suite should reach a mutation")
    }

    async fn health(&self) -> Result<(), ConnectorError> {
        Ok(())
    }
}
