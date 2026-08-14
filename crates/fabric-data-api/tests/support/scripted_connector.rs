//! A connector whose answers the test dictates.
//!
//! [`RecordingConnector`](super::RecordingConnector) exists to capture what
//! *reaches* a connector. This one exists for the opposite question — what a
//! connector may hand *back* — and covers the two shapes that fixture cannot
//! produce:
//!
//! - rows returned from `mutate`, as a backend implementing `RETURNING` does.
//!   Without one, the write path's projection could not be exercised at all.
//! - an [`ConnectorError::Unsupported`] carrying the physical detail a
//!   *translating* connector records, which is what must not reach a body.
//! - an arbitrary [`ConnectorError`], so the transport variants — which differ
//!   only in *where in the HTTP exchange* the call broke — can be driven
//!   through the assembled router.

use std::sync::Arc;

use async_trait::async_trait;
use fabric_connector::{
    CollectionName, CollectionSchema, ConnectorCapabilities, ConnectorError, ConnectorId, ConnectorSchema,
    DataConnector, ExecutionTarget, MutationOutcome, MutationSpec, QueryOutcome, QuerySpec, RefusalDetail,
    Row, UnsupportedFeature,
};
use serde_json::Value;

use crate::support::field;

/// A row with every column a shared table really has: the two the restricted
/// catalogue entry exposes, one it does not, and the tenant discriminator.
pub fn wide_row(id: i64, name: &str) -> Row {
    Row::new()
        .with(field("id"), Value::from(id))
        .with(field("name"), Value::String(name.to_owned()))
        .with(field("salary"), Value::from(190_000))
        .with(field("tenant_key"), Value::String("tenant-482".to_owned()))
}

/// Answers reads and writes with whatever the test configured.
pub struct ScriptedConnector {
    id: ConnectorId,
    capabilities: ConnectorCapabilities,
    schema: ConnectorSchema,
    rows: Vec<Row>,
    refusal: Option<(UnsupportedFeature, RefusalDetail)>,
    failure: Option<fn() -> ConnectorError>,
}

impl ScriptedConnector {
    /// Returns `rows` from reads *and* from writes.
    pub fn returning(rows: Vec<Row>) -> Arc<Self> {
        Arc::new(Self {
            rows,
            refusal: None,
            ..Self::base()
        })
    }

    /// Refuses every operation as unsupported, naming this capability.
    pub fn refusing(feature: UnsupportedFeature) -> Arc<Self> {
        Arc::new(Self {
            refusal: Some((feature, RefusalDetail::none())),
            ..Self::base()
        })
    }

    /// The same, plus the operator-only detail a translating connector records
    /// — the collection, field, or procedure the refusal was raised over.
    pub fn refusing_with_detail(feature: UnsupportedFeature, detail: &str) -> Arc<Self> {
        Arc::new(Self {
            refusal: Some((feature, RefusalDetail::new(detail))),
            ..Self::base()
        })
    }

    /// Fails every operation with whatever `build` produces.
    ///
    /// A factory rather than a stored error because `ConnectorError` is not
    /// `Clone`: its transport variants box an arbitrary source, and a test that
    /// drives several requests needs a fresh one each time.
    pub fn failing(build: fn() -> ConnectorError) -> Arc<Self> {
        Arc::new(Self {
            failure: Some(build),
            ..Self::base()
        })
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
            rows: Vec::new(),
            refusal: None,
            failure: None,
        }
    }

    fn refusal<T>(&self) -> Option<Result<T, ConnectorError>> {
        if let Some(build) = self.failure {
            return Some(Err(build()));
        }

        self.refusal.as_ref().map(|(feature, detail)| {
            Err(ConnectorError::Unsupported {
                feature: *feature,
                detail: detail.clone(),
            })
        })
    }
}

#[async_trait]
impl DataConnector for ScriptedConnector {
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
        self.refusal()
            .unwrap_or_else(|| Ok(QueryOutcome::from_rows(self.rows.clone())))
    }

    async fn mutate(&self, _: &ExecutionTarget, _: &MutationSpec) -> Result<MutationOutcome, ConnectorError> {
        self.refusal()
            .unwrap_or_else(|| Ok(MutationOutcome::affected(1).with_rows(self.rows.clone())))
    }

    async fn health(&self) -> Result<(), ConnectorError> {
        Ok(())
    }
}
