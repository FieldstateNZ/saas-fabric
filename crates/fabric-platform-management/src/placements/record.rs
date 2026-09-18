//! One tenant's placement: the fact, once the selector has decided it.

use fabric_core::{DataSourceId, LogicalDataSourceName, TenantId};
use fabric_runtime_publication::IsolationModelDocument;

/// Where one tenant's logical data source landed, and how it is isolated.
///
/// # This is the record ADR 0023 part 2 makes the authority
///
/// Publication reads this and copies it into the tenant's runtime binding;
/// it never recomputes it. That is what keeps a rule that could change --
/// which data source is least loaded, which one still accepts tenants --
/// from producing a different answer for a tenant whose rows were written
/// under the first one. A break-glass edit to `placements.yaml` is honoured
/// as written for the same reason: the fact is the record, not a formula
/// run again.
///
/// `isolation` is the wire's own [`IsolationModelDocument`], not a second
/// declaration of the same three shapes, so a hand-editable
/// `placements.yaml` can never disagree with what publication would copy
/// from it.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PlacementRecord {
    /// Which tenant this is. The client id, validated as a `TenantId` --
    /// see `docs/architecture/client-desired-state.md`'s `spec.data`
    /// section for why the record is what the runtime reads.
    pub tenant: TenantId,

    /// Which of the tenant's logical data sources this places.
    pub logical: LogicalDataSourceName,

    /// The declared data source this tenant's `logical` landed on.
    pub data_source: DataSourceId,

    /// How this tenant's rows are kept apart from another's on
    /// `data_source`.
    pub isolation: IsolationModelDocument,

    /// When this was placed, RFC 3339.
    pub placed_at: String,
}
