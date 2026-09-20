//! One tenant's placement: the fact, once the selector has decided it.

use fabric_core::{BindingRevision, DataSourceId, LogicalDataSourceName, TenantId};
use fabric_runtime_publication::IsolationModelDocument;

/// The revision a freshly selected record starts at.
///
/// `select::pick` is the only production code that mints a `PlacementRecord`
/// from nothing, and it always starts one at 1 -- the same floor
/// `data-sources.yaml`'s own `revision` starts a new declaration at. Named
/// so the constant reads at every call site instead of a bare `1`.
pub(crate) const FIRST_REVISION: BindingRevision = BindingRevision::new(1);

/// [`FIRST_REVISION`], for `#[serde(default = ...)]` -- which needs a path
/// to a function, not a constant.
fn first_revision() -> BindingRevision {
    FIRST_REVISION
}

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

    /// This record's own revision -- the same kind of number
    /// `data-sources.yaml` carries per entry, and for the same reason: a
    /// break-glass edit that changes what this record says (which data
    /// source, which isolation) must bump it. Publication sums a tenant's
    /// records into that tenant's runtime binding revision (ADR 0023 part
    /// 4, D1), so the runtime ignores a stale republish of an older edit
    /// and refuses a same-revision publication whose bytes disagree,
    /// exactly as it already does for a data source.
    ///
    /// Defaults to `1` on the way in, so every `placements.yaml` written
    /// before this field existed still parses, and reads as revision 1 --
    /// nothing edited it, so nothing has moved it.
    #[serde(default = "first_revision")]
    pub revision: BindingRevision,

    /// The declared data source this tenant's `logical` landed on.
    pub data_source: DataSourceId,

    /// How this tenant's rows are kept apart from another's on
    /// `data_source`.
    pub isolation: IsolationModelDocument,

    /// When this was placed, RFC 3339.
    pub placed_at: String,
}
